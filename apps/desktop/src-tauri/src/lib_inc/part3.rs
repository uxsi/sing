#[tauri::command]
fn core_stop() -> Value {
    let mut state = CORE.lock().expect("core lock");
    if let Some(mut child) = state.child.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
    state.state = "stopped".into();
    state.soft_fail = false;
    // Drop system proxy when core stops so the machine is not left pointing at a dead :1080.
    drop(state);
    let proxy_clear = system_proxy_set(false);
    let mut state = CORE.lock().expect("core lock");
    json!({
        "ok": true,
        "status": state.status(),
        "systemProxy": proxy_clear
    })
}

#[tauri::command]
fn clash_get_proxies(base_url: Option<String>) -> Value {
    match clash_get("/proxies", base_url) {
        Ok(data) => json!({ "ok": true, "data": data }),
        Err(error) => json!({ "ok": false, "error": error, "code": "CORE_UNAVAILABLE" }),
    }
}

#[tauri::command]
fn clash_get_connections(base_url: Option<String>) -> Value {
    match clash_get("/connections", base_url) {
        Ok(data) => json!({ "ok": true, "data": data }),
        Err(error) => json!({ "ok": false, "error": error, "code": "CORE_UNAVAILABLE" }),
    }
}

#[tauri::command]
fn clash_delay(name: String, base_url: Option<String>) -> Value {
    if name.trim().is_empty() {
        return json!({ "ok": false, "error": "proxy name is required", "code": "INVALID" });
    }
    let encoded = urlencoding_encode(&name);
    let path = format!(
        "/proxies/{encoded}/delay?timeout=5000&url=http%3A%2F%2Fwww.gstatic.com%2Fgenerate_204"
    );
    match clash_get(&path, base_url) {
        Ok(data) => {
            let delay = data.get("delay").and_then(|v| v.as_u64());
            json!({ "ok": true, "data": { "name": name, "delay": delay, "raw": data } })
        }
        Err(error) => json!({ "ok": false, "error": error, "code": "CORE_UNAVAILABLE" }),
    }
}

#[tauri::command]
fn validate_config(text: String) -> Value {
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(_)) => json!({ "ok": true, "errors": [] }),
        Ok(_) => json!({ "ok": false, "errors": [{ "path": "$", "message": "Config must be a JSON object" }] }),
        Err(err) => json!({ "ok": false, "errors": [{ "path": "$", "message": format!("Invalid JSON: {err}") }] }),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HealthClassifyArgs {
    input: Option<Value>,
    fixture_text: Option<String>,
}

#[tauri::command]
fn health_classify(args: HealthClassifyArgs) -> Value {
    let input = if let Some(v) = args.input {
        v
    } else if let Some(text) = args.fixture_text {
        match serde_json::from_str::<Value>(&text) {
            Ok(v) => v,
            Err(err) => return json!({ "ok": false, "error": format!("invalid fixture JSON: {err}") }),
        }
    } else {
        return json!({ "ok": false, "error": "input or fixtureText required" });
    };

    let mut warnings: Vec<Value> = Vec::new();
    let dual_tun = input
        .get("interfaces")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|i| {
                    i.get("name")
                        .and_then(|n| n.as_str())
                        .map(|n| n.starts_with("utun") || n.starts_with("tun"))
                        .unwrap_or(false)
                })
                .count()
                >= 2
        })
        .unwrap_or(false);
    if dual_tun {
        warnings.push(json!({
            "code": "DUAL_TUN",
            "level": "warn",
            "message": "Multiple TUN interfaces detected"
        }));
    }

    let dirty = input
        .get("dnsAnswers")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter().any(|ans| {
                ans.get("address")
                    .and_then(|a| a.as_str())
                    .map(|ip| ip.starts_with("100.12."))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);
    if dirty {
        warnings.push(json!({
            "code": "DIRTY_DNS",
            "level": "warn",
            "message": "DNS answer looks like dirty / captive segment"
        }));
    }

    json!({
        "ok": warnings.is_empty(),
        "warnings": warnings,
        "source": "tauri-lite"
    })
}

/// Minimal URL-encode for path segments (proxy names).
fn urlencoding_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urlencoding_spaces_and_ascii() {
        assert_eq!(urlencoding_encode("proxy"), "proxy");
        assert_eq!(urlencoding_encode("a b"), "a%20b");
    }

    #[test]
    fn validate_rejects_non_object() {
        let v = validate_config("[]".into());
        assert_eq!(v["ok"], false);
    }

    #[test]
    fn core_status_starts_stopped() {
        let v = core_status();
        assert_eq!(v["ok"], true);
        assert_eq!(v["status"]["state"], "stopped");
    }
}



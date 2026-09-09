fn clash_get(path: &str, base: Option<String>) -> Result<Value, String> {
    let base = base.unwrap_or_else(|| DEFAULT_CLASH_API.to_string());
    let base = base.trim_end_matches('/');
    let url = format!("{base}{path}");
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;
    let res = client.get(&url).header("Accept", "application/json").send();
    match res {
        Ok(resp) => {
            if !resp.status().is_success() {
                return Err(format!("clash_api HTTP {}", resp.status()));
            }
            resp.json::<Value>().map_err(|e| e.to_string())
        }
        Err(err) => {
            let message = err.to_string();
            Err(format!(
                "clash_api unavailable at {base} (core not running?): {message}"
            ))
        }
    }
}



fn runtime_dir() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("sing-desktop");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn stderr_tail(child: &mut Child) -> String {
    let mut err_tail = String::new();
    if let Some(mut stderr) = child.stderr.take() {
        let mut buf = String::new();
        let _ = stderr.read_to_string(&mut buf);
        let trimmed = buf.trim();
        if !trimmed.is_empty() {
            let bytes = trimmed.as_bytes();
            let start = bytes.len().saturating_sub(1500);
            err_tail = String::from_utf8_lossy(&bytes[start..]).into_owned();
        }
    }
    err_tail
}

fn looks_like_tun_permission_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("tun")
        && (lower.contains("operation not permitted")
            || lower.contains("permission denied")
            || lower.contains("configure tun interface"))
}


/// Apply office/abroad mode tweaks and write a temp config for sing-box to run.
/// manual → copy as-is (still materialize so TUN stripping can chain off it).

fn force_office_selector_direct() {
    // Best-effort: clash API may need a moment after listen.
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(150));
        let client = match reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
        {
            Ok(c) => c,
            Err(_) => return,
        };
        let url = format!("{DEFAULT_CLASH_API}/proxies/final");
        let body = serde_json::json!({ "name": "direct" });
        match client.put(&url).json(&body).send() {
            Ok(resp) if resp.status().is_success() => return,
            _ => continue,
        }
    }
}

fn write_mode_patched_config(src: &Path, mode: &str) -> Result<PathBuf, String> {
    let raw = std::fs::read_to_string(src).map_err(|e| e.to_string())?;
    let mut value: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let mode = mode.trim().to_ascii_lowercase();

    if mode == "office" {
        // Bias selector tag=final default → direct (UI promises this).
        if let Some(outs) = value.get_mut("outbounds").and_then(|v| v.as_array_mut()) {
            for ob in outs.iter_mut() {
                if ob.get("type").and_then(|t| t.as_str()) == Some("selector")
                    && ob.get("tag").and_then(|t| t.as_str()) == Some("final")
                {
                    let has_direct = ob
                        .get("outbounds")
                        .and_then(|o| o.as_array())
                        .map(|arr| arr.iter().any(|x| x.as_str() == Some("direct")))
                        .unwrap_or(false);
                    if has_direct {
                        ob.as_object_mut()
                            .ok_or_else(|| "final outbound not an object".to_string())?
                            .insert("default".into(), Value::String("direct".into()));
                    }
                }
            }
        }
        // GitHub / ssh / git → direct before geosite-github→proxy
        if let Some(rules) = value
            .pointer_mut("/route/rules")
            .and_then(|v| v.as_array_mut())
        {
            let mut insert_at = 0usize;
            while insert_at < rules.len() {
                let r = &rules[insert_at];
                let action = r.get("action").and_then(|a| a.as_str()).unwrap_or("");
                let proto = r.get("protocol");
                let is_dns = action == "sniff"
                    || action == "hijack-dns"
                    || proto.and_then(|p| p.as_str()) == Some("dns")
                    || proto
                        .and_then(|p| p.as_array())
                        .map(|a| a.iter().any(|x| x.as_str() == Some("dns")))
                        .unwrap_or(false);
                if is_dns {
                    insert_at += 1;
                    continue;
                }
                break;
            }
            let protect = vec![
                serde_json::json!({
                    "domain_suffix": ["github.com", "githubusercontent.com"],
                    "outbound": "direct"
                }),
                serde_json::json!({
                    "domain_suffix": ["woa.com", "oa.com"],
                    "outbound": "direct"
                }),
                serde_json::json!({
                    "process_name": ["ssh", "git"],
                    "outbound": "direct"
                }),
            ];
            for (i, rule) in protect.into_iter().enumerate() {
                rules.insert(insert_at + i, rule);
            }
        }
        // coexistence with company tunnel
        if let Some(inbounds) = value.get_mut("inbounds").and_then(|v| v.as_array_mut()) {
            for ib in inbounds.iter_mut() {
                if ib.get("type").and_then(|t| t.as_str()) == Some("tun") {
                    if let Some(obj) = ib.as_object_mut() {
                        obj.insert("strict_route".into(), Value::Bool(false));
                    }
                }
            }
        }
        // Dead proxy node must not break DNS for direct sites (Google etc.) under system proxy.
        if value.get("dns").and_then(|d| d.get("servers")).is_some() {
            if let Some(dns) = value.get_mut("dns").and_then(|d| d.as_object_mut()) {
                let has_local = dns
                    .get("servers")
                    .and_then(|s| s.as_array())
                    .map(|arr| {
                        arr.iter().any(|s| s.get("tag").and_then(|t| t.as_str()) == Some("dns-local"))
                    })
                    .unwrap_or(false);
                if has_local {
                    dns.insert("final".into(), Value::String("dns-local".into()));
                }
            }
        }
        // cache.db remembers clash selector "now=proxy" and overrides config default.
        if let Some(exp) = value.get_mut("experimental").and_then(|e| e.as_object_mut()) {
            exp.insert(
                "cache_file".into(),
                serde_json::json!({ "enabled": false }),
            );
        }
    } else if mode == "abroad" {
        if let Some(inbounds) = value.get_mut("inbounds").and_then(|v| v.as_array_mut()) {
            for ib in inbounds.iter_mut() {
                if ib.get("type").and_then(|t| t.as_str()) == Some("tun") {
                    if let Some(obj) = ib.as_object_mut() {
                        obj.insert("strict_route".into(), Value::Bool(true));
                    }
                }
            }
        }
    }

    let dir = runtime_dir()?;
    let out = dir.join(format!("baseline-{mode}.json"));
    let body = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    std::fs::write(&out, body + "\n").map_err(|e| e.to_string())?;
    Ok(out)
}

/// Write a temp config with TUN inbounds removed (mixed/HTTP only) for unprivileged macOS.

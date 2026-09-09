use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

const DEFAULT_CLASH_API: &str = "http://127.0.0.1:9090";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CoreStatus {
    state: String,
    binary: Option<String>,
    config_path: Option<String>,
    pid: Option<u32>,
    last_error: Option<String>,
    soft_fail: bool,
}

struct CoreState {
    child: Option<Child>,
    binary: Option<String>,
    config_path: Option<String>,
    last_error: Option<String>,
    soft_fail: bool,
    state: String,
}

impl CoreState {
    fn new() -> Self {
        Self {
            child: None,
            binary: None,
            config_path: None,
            last_error: None,
            soft_fail: false,
            state: "stopped".into(),
        }
    }

    fn status(&mut self) -> CoreStatus {
        if let Some(child) = self.child.as_mut() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    self.child = None;
                    self.state = "crashed".into();
                    self.last_error = Some(format!("core exited: {status}"));
                }
                Ok(None) => {
                    self.state = "running".into();
                }
                Err(err) => {
                    self.last_error = Some(err.to_string());
                }
            }
        }
        CoreStatus {
            state: self.state.clone(),
            binary: self.binary.clone(),
            config_path: self.config_path.clone(),
            pid: self.child.as_ref().and_then(|c| Some(c.id())),
            last_error: self.last_error.clone(),
            soft_fail: self.soft_fail,
        }
    }
}

static CORE: Lazy<Mutex<CoreState>> = Lazy::new(|| Mutex::new(CoreState::new()));

fn detect_sing_box() -> Option<String> {
    let candidates = ["sing-box", "singbox"];
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            for name in candidates {
                let full = dir.join(name);
                if full.is_file() {
                    return Some(full.to_string_lossy().into_owned());
                }
            }
        }
    }
    for extra in ["/usr/local/bin", "/opt/homebrew/bin"] {
        for name in candidates {
            let full = Path::new(extra).join(name);
            if full.is_file() {
                return Some(full.to_string_lossy().into_owned());
            }
        }
    }
    None
}

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

#[tauri::command]
fn core_status() -> Value {
    let mut state = CORE.lock().expect("core lock");
    json!({ "ok": true, "status": state.status() })
}

#[tauri::command]
fn core_start(config_path: String) -> Value {
    let mut state = CORE.lock().expect("core lock");
    if let Some(mut child) = state.child.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let path = PathBuf::from(&config_path);
    state.config_path = Some(path.to_string_lossy().into_owned());
    state.soft_fail = false;
    state.last_error = None;

    let Some(binary) = detect_sing_box() else {
        state.state = "missing_binary".into();
        state.soft_fail = true;
        state.binary = None;
        state.last_error = Some("sing-box binary not found".into());
        return json!({
            "ok": false,
            "softFail": true,
            "error": "sing-box binary not found",
            "status": state.status()
        });
    };
    state.binary = Some(binary.clone());

    match Command::new(&binary)
        .args(["run", "-c", &config_path])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => {
            state.child = Some(child);
            state.state = "running".into();
            json!({ "ok": true, "status": state.status() })
        }
        Err(err) => {
            state.state = "crashed".into();
            state.last_error = Some(err.to_string());
            json!({
                "ok": false,
                "error": err.to_string(),
                "status": state.status()
            })
        }
    }
}

#[tauri::command]
fn core_stop() -> Value {
    let mut state = CORE.lock().expect("core lock");
    if let Some(mut child) = state.child.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
    state.state = "stopped".into();
    state.soft_fail = false;
    json!({ "ok": true, "status": state.status() })
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            core_status,
            core_start,
            core_stop,
            clash_get_proxies,
            clash_get_connections,
            clash_delay,
            validate_config,
            health_classify
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

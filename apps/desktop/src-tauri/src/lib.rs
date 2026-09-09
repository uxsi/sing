use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Read;
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

fn candidate_exists(path: &Path) -> Option<String> {
    if path.is_file() {
        Some(path.to_string_lossy().into_owned())
    } else {
        None
    }
}

fn detect_sing_box() -> Option<String> {
    // 1) Next to the running executable (Tauri externalBin / packaged app)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in ["sing-box", "singbox"] {
                if let Some(p) = candidate_exists(&dir.join(name)) {
                    return Some(p);
                }
            }
            // cargo tauri dev often keeps externalBin as sing-box-<triple> beside the exe
            if let Ok(rd) = std::fs::read_dir(dir) {
                for ent in rd.flatten() {
                    let name = ent.file_name();
                    let name = name.to_string_lossy();
                    if name == "sing-box" || name.starts_with("sing-box-") {
                        if let Some(p) = candidate_exists(&ent.path()) {
                            return Some(p);
                        }
                    }
                }
            }
        }
    }

    // 2) Dev: apps/desktop/src-tauri/binaries/sing-box-<triple>
    if let Ok(cwd) = std::env::current_dir() {
        let triples = [
            "aarch64-apple-darwin",
            "x86_64-apple-darwin",
            "aarch64-unknown-linux-gnu",
            "x86_64-unknown-linux-gnu",
        ];
        let mut search_roots: Vec<PathBuf> = vec![cwd.clone()];
        if let Some(root) = find_repo_root(&cwd) {
            search_roots.push(root.join("apps/desktop/src-tauri"));
            search_roots.push(root);
        }
        // also walk up a few levels from cwd for src-tauri
        for ancestor in cwd.ancestors().take(6) {
            search_roots.push(ancestor.to_path_buf());
        }
        for root in search_roots {
            let bin_dir = if root.join("binaries").is_dir() {
                root.join("binaries")
            } else if root.join("apps/desktop/src-tauri/binaries").is_dir() {
                root.join("apps/desktop/src-tauri/binaries")
            } else {
                continue;
            };
            for triple in triples {
                if let Some(p) = candidate_exists(&bin_dir.join(format!("sing-box-{triple}"))) {
                    return Some(p);
                }
            }
            if let Some(p) = candidate_exists(&bin_dir.join("sing-box")) {
                return Some(p);
            }
            // repo-root bin/ (docs)
            if let Some(p) = candidate_exists(&root.join("bin/sing-box")) {
                return Some(p);
            }
        }
    }

    // 3) PATH + common Homebrew locations (fallback)
    let candidates = ["sing-box", "singbox"];
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            for name in candidates {
                let full = dir.join(name);
                if let Some(p) = candidate_exists(&full) {
                    return Some(p);
                }
            }
        }
    }
    for extra in ["/usr/local/bin", "/opt/homebrew/bin"] {
        for name in candidates {
            let full = Path::new(extra).join(name);
            if let Some(p) = candidate_exists(&full) {
                return Some(p);
            }
        }
    }
    None
}


fn is_repo_root(dir: &Path) -> bool {
    dir.join("configs/examples").is_dir() && dir.join("apps/desktop").is_dir()
}

fn find_repo_root(start: &Path) -> Option<PathBuf> {
    for ancestor in start.ancestors() {
        if is_repo_root(ancestor) {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

/// Resolve config paths for Tauri: absolute paths unchanged; relative paths try
/// cwd first, then walk up to the monorepo root (cwd is often `apps/desktop/src-tauri`).
fn resolve_config_path(input: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(input);
    if path.as_os_str().is_empty() {
        return Err("config path is empty".into());
    }
    if path.is_absolute() {
        return if path.is_file() {
            Ok(path)
        } else {
            Err(format!("config not found: {}", path.display()))
        };
    }

    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let from_cwd = cwd.join(&path);
    if from_cwd.is_file() {
        return Ok(from_cwd);
    }
    if let Some(root) = find_repo_root(&cwd) {
        let from_root = root.join(&path);
        if from_root.is_file() {
            return Ok(from_root);
        }
    }
    Err(format!(
        "config not found for relative path `{input}` (looked from cwd and monorepo root)"
    ))
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
    state.soft_fail = false;
    state.last_error = None;

    let resolved = match resolve_config_path(&config_path) {
        Ok(path) => path,
        Err(err) => {
            state.state = "crashed".into();
            state.config_path = Some(config_path);
            state.last_error = Some(err.clone());
            return json!({
                "ok": false,
                "error": err,
                "status": state.status()
            });
        }
    };
    let resolved_str = resolved.to_string_lossy().into_owned();
    state.config_path = Some(resolved_str.clone());

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
        .args(["run", "-c", &resolved_str])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(mut child) => {
            // Connect used to return success on spawn only; FATAL exits within ~1s
            // (bad config / rule-set / TUN). Probe once so Status is not a surprise.
            std::thread::sleep(Duration::from_millis(700));
            match child.try_wait() {
                Ok(Some(status)) => {
                    let mut err_tail = String::new();
                    if let Some(mut stderr) = child.stderr.take() {
                        let mut buf = String::new();
                        let _ = stderr.read_to_string(&mut buf);
                        // keep last ~1.5KB of log
                        let trimmed = buf.trim();
                        if !trimmed.is_empty() {
                            let bytes = trimmed.as_bytes();
                            let start = bytes.len().saturating_sub(1500);
                            err_tail = String::from_utf8_lossy(&bytes[start..]).into_owned();
                        }
                    }
                    let mut message = format!("core exited during start: {status}");
                    if !err_tail.is_empty() {
                        message = format!("{message}\n{err_tail}");
                    }
                    state.child = None;
                    state.state = "crashed".into();
                    state.last_error = Some(message.clone());
                    json!({
                        "ok": false,
                        "error": message,
                        "status": state.status()
                    })
                }
                Ok(None) => {
                    // Drain stderr in background so the pipe cannot fill and stall core.
                    if let Some(mut stderr) = child.stderr.take() {
                        std::thread::spawn(move || {
                            let mut sink = Vec::new();
                            let _ = stderr.read_to_end(&mut sink);
                        });
                    }
                    state.child = Some(child);
                    state.state = "running".into();
                    json!({ "ok": true, "status": state.status() })
                }
                Err(err) => {
                    let _ = child.kill();
                    let message = err.to_string();
                    state.child = None;
                    state.state = "crashed".into();
                    state.last_error = Some(message.clone());
                    json!({
                        "ok": false,
                        "error": message,
                        "status": state.status()
                    })
                }
            }
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

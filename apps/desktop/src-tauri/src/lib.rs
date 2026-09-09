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
    /// "mixed-only" | "tun" | None when stopped/unknown
    dataplane: Option<String>,
}

struct CoreState {
    child: Option<Child>,
    binary: Option<String>,
    config_path: Option<String>,
    last_error: Option<String>,
    soft_fail: bool,
    state: String,
    dataplane: Option<String>,
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
            dataplane: None,
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
            dataplane: self.dataplane.clone(),
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
        // Prefer proxy path for unmatched traffic (office does the opposite).
        if let Some(outs) = value.get_mut("outbounds").and_then(|v| v.as_array_mut()) {
            for ob in outs.iter_mut() {
                if ob.get("type").and_then(|t| t.as_str()) == Some("selector")
                    && ob.get("tag").and_then(|t| t.as_str()) == Some("final")
                {
                    let has_proxy = ob
                        .get("outbounds")
                        .and_then(|o| o.as_array())
                        .map(|arr| arr.iter().any(|x| x.as_str() == Some("proxy")))
                        .unwrap_or(false);
                    if has_proxy {
                        if let Some(obj) = ob.as_object_mut() {
                            obj.insert("default".into(), Value::String("proxy".into()));
                        }
                    }
                }
            }
        }
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
fn write_config_without_tun(src: &Path) -> Result<PathBuf, String> {
    let raw = std::fs::read_to_string(src).map_err(|e| e.to_string())?;
    let mut value: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    if let Some(inbounds) = value.get_mut("inbounds").and_then(|v| v.as_array_mut()) {
        inbounds.retain(|ib| ib.get("type").and_then(|t| t.as_str()) != Some("tun"));
    }
    if let Some(rules) = value
        .pointer_mut("/route/rules")
        .and_then(|v| v.as_array_mut())
    {
        rules.retain(|rule| {
            match rule.get("inbound") {
                Some(Value::String(s)) => !s.contains("tun"),
                Some(Value::Array(arr)) => !arr.iter().any(|x| {
                    x.as_str().map(|s| s.contains("tun")).unwrap_or(false)
                }),
                _ => true,
            }
        });
    }
    let dir = std::env::temp_dir().join("sing-desktop");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let out = dir.join("baseline-no-tun.json");
    let body = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    std::fs::write(&out, body + "\n").map_err(|e| e.to_string())?;
    Ok(out)
}


fn prefer_mixed_only_dataplane() -> bool {
    // Desktop macOS: no Network Extension yet — start mixed-only and rely on System Proxy.
    // Set SING_TRY_TUN=1 to attempt TUN first (then existing permission fallback).
    if std::env::var("SING_TRY_TUN").ok().as_deref() == Some("1") {
        return false;
    }
    cfg!(target_os = "macos")
}

enum SpawnOutcome {
    Running(Child),
    Exited { status: String, stderr: String },
    SpawnErr(String),
}

fn spawn_and_probe(binary: &str, config: &str) -> SpawnOutcome {
    let cwd = match runtime_dir() {
        Ok(p) => p,
        Err(err) => return SpawnOutcome::SpawnErr(err),
    };
    match Command::new(binary)
        .args(["run", "-c", config])
        .current_dir(&cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(mut child) => {
            std::thread::sleep(Duration::from_millis(700));
            match child.try_wait() {
                Ok(Some(status)) => {
                    let stderr = stderr_tail(&mut child);
                    SpawnOutcome::Exited {
                        status: status.to_string(),
                        stderr,
                    }
                }
                Ok(None) => {
                    if let Some(mut stderr) = child.stderr.take() {
                        std::thread::spawn(move || {
                            let mut sink = Vec::new();
                            let _ = stderr.read_to_end(&mut sink);
                        });
                    }
                    SpawnOutcome::Running(child)
                }
                Err(err) => {
                    let _ = child.kill();
                    SpawnOutcome::SpawnErr(err.to_string())
                }
            }
        }
        Err(err) => SpawnOutcome::SpawnErr(err.to_string()),
    }
}


#[tauri::command]
fn core_status() -> Value {
    let mut state = CORE.lock().expect("core lock");
    json!({ "ok": true, "status": state.status() })
}

#[tauri::command]
fn core_start(config_path: String, mode: Option<String>) -> Value {
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
    let mode_name = mode.unwrap_or_else(|| "manual".into());
    let patched = match write_mode_patched_config(&resolved, &mode_name) {
        Ok(path) => path,
        Err(err) => {
            state.state = "crashed".into();
            state.config_path = Some(resolved_str);
            state.last_error = Some(format!("mode patch failed: {err}"));
            return json!({
                "ok": false,
                "error": format!("mode patch failed: {err}"),
                "status": state.status()
            });
        }
    };
    let mut resolved_str = patched.to_string_lossy().into_owned();
    let mut dataplane = "tun".to_string();
    if prefer_mixed_only_dataplane() {
        match write_config_without_tun(Path::new(&resolved_str)) {
            Ok(fallback) => {
                resolved_str = fallback.to_string_lossy().into_owned();
                dataplane = "mixed-only".into();
            }
            Err(err) => {
                state.state = "crashed".into();
                state.config_path = Some(resolved_str);
                state.last_error = Some(format!("mixed-only strip failed: {err}"));
                return json!({
                    "ok": false,
                    "error": format!("mixed-only strip failed: {err}"),
                    "status": state.status()
                });
            }
        }
    }
    state.config_path = Some(resolved_str.clone());
    state.dataplane = Some(dataplane.clone());

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

    let first = spawn_and_probe(&binary, &resolved_str);
    match first {
        SpawnOutcome::Running(child) => {
            state.child = Some(child);
            state.state = "running".into();
            let status = state.status();
            let dp = status.dataplane.clone();
            drop(state);
            if mode_name.eq_ignore_ascii_case("office") {
                force_office_selector_direct();
            }
            let abroad_pick = if mode_name.eq_ignore_ascii_case("abroad") {
                Some(abroad_pick_live_node())
            } else {
                None
            };
            let mut out = json!({ "ok": true, "status": status });
            if let Some(dp) = dp {
                out.as_object_mut().unwrap().insert("dataplane".into(), json!(dp));
                if dp == "mixed-only" {
                    out.as_object_mut().unwrap().insert(
                        "warning".into(),
                        json!("dataplane=mixed-only (no TUN on macOS desktop yet) — use System Proxy for app traffic"),
                    );
                }
            }
            if let Some(pick) = abroad_pick {
                out.as_object_mut()
                    .unwrap()
                    .insert("abroadLive".into(), pick);
            }
            return out;
        }
        SpawnOutcome::SpawnErr(message) => {
            state.state = "crashed".into();
            state.last_error = Some(message.clone());
            return json!({
                "ok": false,
                "error": message,
                "status": state.status()
            });
        }
        SpawnOutcome::Exited { status, stderr } => {
            let mut message = format!("core exited during start: {status}");
            if !stderr.is_empty() {
                message = format!("{message}\n{stderr}");
            }
            // Unprivileged macOS cannot configure utun — fall back to mixed-only once.
            if looks_like_tun_permission_error(&message) {
                match write_config_without_tun(Path::new(&resolved_str)) {
                    Ok(fallback) => {
                        let fb = fallback.to_string_lossy().into_owned();
                        state.config_path = Some(fb.clone());
                        match spawn_and_probe(&binary, &fb) {
                            SpawnOutcome::Running(child) => {
                                state.child = Some(child);
                                state.state = "running".into();
                                state.last_error = Some(
                                    "TUN not permitted; started mixed-only fallback (no utun)".into(),
                                );
                                state.dataplane = Some("mixed-only".into());
                                let status = state.status();
                                drop(state);
                                if mode_name.eq_ignore_ascii_case("office") {
                                    force_office_selector_direct();
                                }
                                let abroad_live = if mode_name.eq_ignore_ascii_case("abroad") {
                                    Some(abroad_pick_live_node())
                                } else {
                                    None
                                };
                                let mut out = json!({
                                    "ok": true,
                                    "fallback": "mixed-only",
                                    "dataplane": "mixed-only",
                                    "warning": "TUN not permitted; started mixed-only fallback — enable System Proxy for app traffic",
                                    "status": status
                                });
                                if let Some(pick) = abroad_live {
                                    out.as_object_mut()
                                        .unwrap()
                                        .insert("abroadLive".into(), pick);
                                }
                                return out;
                            }
                            SpawnOutcome::Exited { status, stderr } => {
                                let mut message2 = format!(
                                    "core exited during start (after mixed-only fallback): {status}"
                                );
                                if !stderr.is_empty() {
                                    message2 = format!("{message2}\n{stderr}");
                                }
                                message = format!("{message}\n---\n{message2}");
                            }
                            SpawnOutcome::SpawnErr(err) => {
                                message = format!("{message}\n---\nmixed-only fallback spawn: {err}");
                            }
                        }
                    }
                    Err(err) => {
                        message = format!("{message}\n---\nmixed-only fallback config: {err}");
                    }
                }
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
    state.dataplane = None;
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


fn clash_put_proxy(group: &str, name: &str) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;
    let url = format!(
        "{DEFAULT_CLASH_API}/proxies/{}",
        urlencoding_encode(group)
    );
    let body = serde_json::json!({ "name": name });
    let resp = client
        .put(&url)
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("clash put {group}=>{name} HTTP {}", resp.status()));
    }
    Ok(())
}

fn clash_delay_ms(name: &str) -> Option<u64> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .ok()?;
    let encoded = urlencoding_encode(name);
    let url = format!(
        "{DEFAULT_CLASH_API}/proxies/{encoded}/delay?timeout=5000&url=http%3A%2F%2Fwww.gstatic.com%2Fgenerate_204"
    );
    let resp = client.get(&url).send().ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let data: Value = resp.json().ok()?;
    data.get("delay").and_then(|v| v.as_u64())
}

/// Probe `proxy` selector members; pick first live node; force `final` → `proxy`.
fn abroad_pick_live_node() -> Value {
    // clash API may need a moment after listen
    for _ in 0..12 {
        std::thread::sleep(Duration::from_millis(200));
        let proxies = match clash_get("/proxies", None) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let map = proxies
            .get("proxies")
            .cloned()
            .unwrap_or(proxies);
        let Some(proxy_group) = map.get("proxy") else {
            continue;
        };
        let candidates: Vec<String> = proxy_group
            .get("all")
            .and_then(|a| a.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .filter(|s| {
                        let u = s.to_ascii_uppercase();
                        u != "DIRECT" && u != "REJECT" && u != "PROXY"
                    })
                    .collect()
            })
            .unwrap_or_default();
        if candidates.is_empty() {
            // single named outbound used as proxy group now
            if let Some(now) = proxy_group.get("now").and_then(|v| v.as_str()) {
                let _ = clash_put_proxy("final", "proxy");
                return json!({
                    "ok": true,
                    "picked": now,
                    "probed": 0,
                    "note": "proxy group has no all[]; left now as-is and set final→proxy"
                });
            }
            continue;
        }

        let mut probed = Vec::new();
        let mut picked: Option<String> = None;
        for name in &candidates {
            let delay = clash_delay_ms(name);
            probed.push(json!({ "name": name, "delay": delay }));
            if delay.is_some() && picked.is_none() {
                picked = Some(name.clone());
            }
        }

        if let Some(ref name) = picked {
            if let Err(err) = clash_put_proxy("proxy", name) {
                return json!({ "ok": false, "error": err, "probed": probed });
            }
        }
        if let Err(err) = clash_put_proxy("final", "proxy") {
            return json!({ "ok": false, "error": err, "probed": probed, "picked": picked });
        }

        return if picked.is_some() {
            json!({
                "ok": true,
                "picked": picked,
                "probed": probed,
                "final": "proxy"
            })
        } else {
            json!({
                "ok": false,
                "error": "no live proxy nodes (all delay failed)",
                "probed": probed,
                "final": "proxy",
                "code": "NO_LIVE_NODE"
            })
        };
    }
    json!({
        "ok": false,
        "error": "clash_api proxies unavailable while picking abroad live node",
        "code": "CORE_UNAVAILABLE"
    })
}


#[tauri::command]
fn abroad_pick_live_node_cmd() -> Value {
    abroad_pick_live_node()
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


const MIXED_HOST: &str = "127.0.0.1";
const MIXED_PORT: &str = "1080";

fn is_our_mixed_endpoint(server: &str, port: &str) -> bool {
    let server = server.trim();
    let port = port.trim();
    (server == MIXED_HOST || server.eq_ignore_ascii_case("localhost"))
        && (port == MIXED_PORT || port == "1080")
}


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedProxyState {
    service: String,
    web_enabled: bool,
    web_server: String,
    web_port: String,
    secure_enabled: bool,
    secure_server: String,
    secure_port: String,
    socks_enabled: bool,
    socks_server: String,
    socks_port: String,
}

struct SystemProxyState {
    enabled: bool,
    saved: Vec<SavedProxyState>,
}

impl SystemProxyState {
    fn new() -> Self {
        Self {
            enabled: false,
            saved: Vec::new(),
        }
    }
}

static SYSTEM_PROXY: Lazy<Mutex<SystemProxyState>> =
    Lazy::new(|| Mutex::new(SystemProxyState::new()));

fn run_networksetup(args: &[&str]) -> Result<String, String> {
    let out = Command::new("/usr/sbin/networksetup")
        .args(args)
        .output()
        .map_err(|e| format!("networksetup: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if !out.status.success() {
        let msg = if !stderr.is_empty() { stderr } else { stdout };
        return Err(format!("networksetup {:?} failed: {msg}", args));
    }
    Ok(stdout)
}

fn list_network_services() -> Result<Vec<String>, String> {
    let raw = run_networksetup(&["-listallnetworkservices"])?;
    let mut services = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("An asterisk") {
            continue;
        }
        // Disabled services are prefixed with "* "
        let name = line.strip_prefix("* ").unwrap_or(line).to_string();
        if name.is_empty() {
            continue;
        }
        // Skip known VPN/tun clients that fight system proxy
        let lower = name.to_ascii_lowercase();
        if lower.contains("sfm") || lower.contains("quantumult") || lower.contains("clash") {
            continue;
        }
        services.push(name);
    }
    Ok(services)
}

fn parse_proxy_block(raw: &str) -> (bool, String, String) {
    let mut enabled = false;
    let mut server = String::new();
    let mut port = String::new();
    for line in raw.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Enabled:") {
            enabled = rest.trim().eq_ignore_ascii_case("yes");
        } else if let Some(rest) = line.strip_prefix("Server:") {
            server = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("Port:") {
            port = rest.trim().to_string();
        }
    }
    (enabled, server, port)
}

fn snapshot_service(service: &str) -> Result<SavedProxyState, String> {
    let web = run_networksetup(&["-getwebproxy", service])?;
    let secure = run_networksetup(&["-getsecurewebproxy", service])?;
    let socks = run_networksetup(&["-getsocksfirewallproxy", service])?;
    let (mut web_enabled, web_server, web_port) = parse_proxy_block(&web);
    let (mut secure_enabled, secure_server, secure_port) = parse_proxy_block(&secure);
    let (mut socks_enabled, socks_server, socks_port) = parse_proxy_block(&socks);
    // Treat leftover sing mixed endpoints as "was off" so Off does not restore them.
    if is_our_mixed_endpoint(&web_server, &web_port) {
        web_enabled = false;
    }
    if is_our_mixed_endpoint(&secure_server, &secure_port) {
        secure_enabled = false;
    }
    if is_our_mixed_endpoint(&socks_server, &socks_port) {
        socks_enabled = false;
    }
    Ok(SavedProxyState {
        service: service.to_string(),
        web_enabled,
        web_server,
        web_port,
        secure_enabled,
        secure_server,
        secure_port,
        socks_enabled,
        socks_server,
        socks_port,
    })
}

fn apply_sing_proxy(service: &str) -> Result<(), String> {
    // HTTP/HTTPS only — enabling SOCKS too makes some stacks (Safari + corporate agents)
    // fail closed when :1080 flaps. Mixed inbound still accepts both if needed later.
    run_networksetup(&["-setwebproxy", service, MIXED_HOST, MIXED_PORT])?;
    run_networksetup(&["-setsecurewebproxy", service, MIXED_HOST, MIXED_PORT])?;
    run_networksetup(&["-setwebproxystate", service, "on"])?;
    run_networksetup(&["-setsecurewebproxystate", service, "on"])?;
    // Ensure SOCKS is not left pointing at us from a prior toggle.
    let _ = run_networksetup(&["-setsocksfirewallproxystate", service, "off"]);
    let _ = run_networksetup(&[
        "-setproxybypassdomains",
        service,
        "127.0.0.1",
        "localhost",
        "*.local",
        "169.254.0.0/16",
        "10.0.0.0/8",
        "172.16.0.0/12",
        "192.168.0.0/16",
        // Corporate intranet (Tencent OA / iOA) must not enter mixed.
        "*.woa.com",
        "woa.com",
        "*.oa.com",
        "oa.com",
        "*.ioa.tencent.com",
        "ioa.tencent.com",
    ]);
    Ok(())
}

fn mixed_port_listening() -> bool {
    use std::net::TcpStream;
    use std::time::Duration;
    TcpStream::connect_timeout(
        &format!("{MIXED_HOST}:{MIXED_PORT}")
            .parse()
            .unwrap_or_else(|_| format!("127.0.0.1:{MIXED_PORT}").parse().unwrap()),
        Duration::from_millis(400),
    )
    .is_ok()
}

fn restore_service(saved: &SavedProxyState) -> Result<(), String> {
    let svc = saved.service.as_str();
    // Never "restore" back onto our own mixed endpoint — that leaves System Proxy
    // looking still on after Off when a previous run left 127.0.0.1:1080 in place.
    let web_ours = is_our_mixed_endpoint(&saved.web_server, &saved.web_port);
    let secure_ours = is_our_mixed_endpoint(&saved.secure_server, &saved.secure_port);
    let socks_ours = is_our_mixed_endpoint(&saved.socks_server, &saved.socks_port);

    if saved.web_enabled && !saved.web_server.is_empty() && !web_ours {
        run_networksetup(&[
            "-setwebproxy",
            svc,
            &saved.web_server,
            if saved.web_port.is_empty() { "0" } else { &saved.web_port },
        ])?;
        run_networksetup(&["-setwebproxystate", svc, "on"])?;
    } else {
        let _ = run_networksetup(&["-setwebproxystate", svc, "off"]);
    }
    if saved.secure_enabled && !saved.secure_server.is_empty() && !secure_ours {
        run_networksetup(&[
            "-setsecurewebproxy",
            svc,
            &saved.secure_server,
            if saved.secure_port.is_empty() {
                "0"
            } else {
                &saved.secure_port
            },
        ])?;
        run_networksetup(&["-setsecurewebproxystate", svc, "on"])?;
    } else {
        let _ = run_networksetup(&["-setsecurewebproxystate", svc, "off"]);
    }
    if saved.socks_enabled && !saved.socks_server.is_empty() && !socks_ours {
        run_networksetup(&[
            "-setsocksfirewallproxy",
            svc,
            &saved.socks_server,
            if saved.socks_port.is_empty() {
                "0"
            } else {
                &saved.socks_port
            },
        ])?;
        run_networksetup(&["-setsocksfirewallproxystate", svc, "on"])?;
    } else {
        let _ = run_networksetup(&["-setsocksfirewallproxystate", svc, "off"]);
    }
    Ok(())
}

fn disable_sing_proxy_on_service(service: &str) -> Result<(), String> {
    let _ = run_networksetup(&["-setwebproxystate", service, "off"]);
    let _ = run_networksetup(&["-setsecurewebproxystate", service, "off"]);
    let _ = run_networksetup(&["-setsocksfirewallproxystate", service, "off"]);
    Ok(())
}

#[tauri::command]
fn system_proxy_status() -> Value {
    let state = SYSTEM_PROXY.lock().expect("system proxy lock");
    json!({
        "ok": true,
        "enabled": state.enabled,
        "host": MIXED_HOST,
        "port": MIXED_PORT.parse::<u16>().unwrap_or(1080),
        "services": state.saved.iter().map(|s| &s.service).collect::<Vec<_>>(),
    })
}

#[tauri::command]
fn system_proxy_set(enabled: bool) -> Value {
    #[cfg(not(target_os = "macos"))]
    {
        return json!({
            "ok": false,
            "error": "system proxy toggle is macOS-only for now",
            "enabled": false
        });
    }
    #[cfg(target_os = "macos")]
    {
        let mut state = SYSTEM_PROXY.lock().expect("system proxy lock");
        if enabled {
            if state.enabled {
                return json!({
                    "ok": true,
                    "enabled": true,
                    "host": MIXED_HOST,
                    "port": 1080,
                    "services": state.saved.iter().map(|s| &s.service).collect::<Vec<_>>(),
                });
            }
            if !mixed_port_listening() {
                return json!({
                    "ok": false,
                    "error": format!(
                        "mixed inbound {MIXED_HOST}:{MIXED_PORT} is not listening — Connect core first"
                    ),
                    "enabled": false
                });
            }
            let services = match list_network_services() {
                Ok(s) if !s.is_empty() => s,
                Ok(_) => {
                    return json!({ "ok": false, "error": "no network services found", "enabled": false });
                }
                Err(err) => return json!({ "ok": false, "error": err, "enabled": false }),
            };
            // Prefer Wi-Fi / Ethernet; still snapshot all usable services
            let mut saved = Vec::new();
            let mut applied = Vec::new();
            let mut errors = Vec::new();
            for svc in &services {
                match snapshot_service(svc) {
                    Ok(snap) => {
                        match apply_sing_proxy(svc) {
                            Ok(()) => {
                                applied.push(svc.clone());
                                saved.push(snap);
                            }
                            Err(err) => errors.push(format!("{svc}: {err}")),
                        }
                    }
                    Err(err) => errors.push(format!("{svc}: {err}")),
                }
            }
            if applied.is_empty() {
                return json!({
                    "ok": false,
                    "error": format!("failed to set system proxy: {}", errors.join("; ")),
                    "enabled": false
                });
            }
            state.enabled = true;
            state.saved = saved;
            json!({
                "ok": true,
                "enabled": true,
                "host": MIXED_HOST,
                "port": 1080,
                "services": applied,
                "warnings": errors,
            })
        } else {
            // Always clear OS proxies on Off — even if in-memory state was lost (HMR / restart)
            // while React still showed On, or macOS was left pointing at :1080.
            let mut errors = Vec::new();
            if !state.saved.is_empty() {
                for snap in &state.saved {
                    if let Err(err) = restore_service(snap) {
                        let _ = disable_sing_proxy_on_service(&snap.service);
                        errors.push(format!("{}: {err}", snap.service));
                    }
                }
            } else if let Ok(services) = list_network_services() {
                for svc in services {
                    let _ = disable_sing_proxy_on_service(&svc);
                }
            }
            state.enabled = false;
            state.saved.clear();
            json!({
                "ok": true,
                "enabled": false,
                "host": MIXED_HOST,
                "port": 1080,
                "warnings": errors,
            })
        }
    }
}


#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|_app| {
            // Dev/QA helper: SING_BOOTSTRAP=1 auto Connect(office) + System Proxy On.
            if let Ok(boot) = std::env::var("SING_BOOTSTRAP") {
                if boot == "1" || boot == "cycle" {
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(1800));
                        let started = core_start(
                            "configs/examples/baseline.json".into(),
                            Some("office".into()),
                        );
                        eprintln!("[SING_BOOTSTRAP] core_start => {started}");
                        std::thread::sleep(Duration::from_millis(900));
                        let proxy = system_proxy_set(true);
                        eprintln!("[SING_BOOTSTRAP] system_proxy_set(on) => {proxy}");
                        if boot == "cycle" {
                            std::thread::sleep(Duration::from_millis(2500));
                            let off = system_proxy_set(false);
                            eprintln!("[SING_BOOTSTRAP] system_proxy_set(off) => {off}");
                        }
                    });
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            core_status,
            core_start,
            core_stop,
            clash_get_proxies,
            clash_get_connections,
            clash_delay,
            abroad_pick_live_node_cmd,
            validate_config,
            health_classify,
            system_proxy_status,
            system_proxy_set
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

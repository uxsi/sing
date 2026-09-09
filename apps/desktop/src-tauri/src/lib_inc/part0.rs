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


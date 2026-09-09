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
    let resolved_str = patched.to_string_lossy().into_owned();
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

    let first = spawn_and_probe(&binary, &resolved_str);
    match first {
        SpawnOutcome::Running(child) => {
            state.child = Some(child);
            state.state = "running".into();
            let status = state.status();
            drop(state);
            if mode_name.eq_ignore_ascii_case("office") {
                force_office_selector_direct();
            }
            return json!({ "ok": true, "status": status });
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
                                let status = state.status();
                                drop(state);
                                if mode_name.eq_ignore_ascii_case("office") {
                                    force_office_selector_direct();
                                }
                                return json!({
                                    "ok": true,
                                    "fallback": "mixed-only",
                                    "warning": "TUN not permitted; started mixed-only fallback",
                                    "status": status
                                });
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


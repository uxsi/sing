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
            validate_config,
            health_classify,
            system_proxy_status,
            system_proxy_set
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

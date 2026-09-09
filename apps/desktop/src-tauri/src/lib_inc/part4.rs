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


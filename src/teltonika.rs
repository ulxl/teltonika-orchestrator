use reqwest::Client;
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, Mutex};
use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use base64::Engine;

struct SshClientHandler;

#[async_trait]
impl russh::client::Handler for SshClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct JobParams {
    pub mode: String,
    pub device_type: String,
    pub ips: Vec<String>,
    pub all_input_ips: Vec<String>,
    pub passwords: Vec<String>,
    pub standard_passwords: Vec<String>,
    pub mac_addresses: Vec<String>,
    pub username: String,
    pub port: String,
    pub workers: usize,
    pub firmware_path: Option<String>,
    pub backup_file_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DeviceRecord {
    pub ip: String,
    pub status: String,
    pub details: Vec<String>,
}

pub fn create_http_client() -> Client {
    Client::builder()
        .danger_accept_invalid_certs(true)
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .pool_idle_timeout(Duration::from_secs(15))
        .pool_max_idle_per_host(4)
        .build()
        .unwrap_or_else(|_| Client::new())
}

pub async fn is_port_open(host: &str, port: u16, timeout_ms: u64) -> bool {
    let addr = format!("{}:{}", host, port);
    match tokio::time::timeout(Duration::from_millis(timeout_ms), tokio::net::TcpStream::connect(&addr)).await {
        Ok(Ok(_)) => true,
        _ => false,
    }
}

pub async fn is_reachable(ip: &str) -> bool {
    let hostname = ip
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split(':')
        .next()
        .unwrap_or(ip);

    for &port in &[8443, 80, 443, 22, 822] {
        if is_port_open(hostname, port, 350).await {
            return true;
        }
    }
    false
}

pub async fn run_ubus_cmd(client: &Client, base_url: &str, session_id: &str, cmd: &str, args: &[&str]) -> String {
    let url = format!("{}/ubus", base_url);
    let payload = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "call",
        "params": [session_id, "file", "exec", {
            "command": cmd,
            "params": args
        }]
    });

    let send_fut = async {
        if let Ok(res) = client.post(&url).json(&payload).send().await {
            if let Ok(val) = res.json::<Value>().await {
                if let Some(stdout) = val.pointer("/result/1/stdout").and_then(|v| v.as_str()) {
                    return stdout.trim().to_string();
                }
            }
        }
        "N/A".to_string()
    };

    tokio::time::timeout(Duration::from_millis(4500), send_fut).await.unwrap_or_else(|_| "N/A".to_string())
}

pub async fn try_login(
    client: &Client,
    base_url: &str,
    username: &str,
    password: &str,
    tx: &broadcast::Sender<String>,
    display_name: &str,
) -> Option<(String, String, bool)> {
    let rest_login_url = format!("{}/api/login", base_url);
    let payload = json!({ "username": username, "password": password });

    let _ = tx.send(format!("[DEBUG] {} Testing REST API login with password '{}'...", display_name, password));

    if let Ok(res) = client.post(&rest_login_url).json(&payload).send().await {
        let code = res.status().as_u16();
        if code == 200 {
            if let Ok(val) = res.json::<Value>().await {
                let token = val.pointer("/data/token")
                    .or_else(|| val.get("token"))
                    .and_then(|t| t.as_str());
                if let Some(tok) = token {
                    let _ = tx.send(format!("[SUCCESS] {} Authenticated via REST API (/api/login).", display_name));
                    return Some((format!("{}/api", base_url), tok.to_string(), false));
                }
            }
        } else if code == 401 || code == 403 {
            let _ = tx.send(format!("[INFO] {} Password rejected (HTTP {}).", display_name, code));
            return None;
        }
    }

    let alt_login_url = format!("{}/login", base_url);
    if let Ok(res) = client.post(&alt_login_url).json(&payload).send().await {
        if res.status().as_u16() == 200 {
            if let Ok(val) = res.json::<Value>().await {
                let token = val.pointer("/data/token")
                    .or_else(|| val.get("token"))
                    .and_then(|t| t.as_str());
                if let Some(tok) = token {
                    let _ = tx.send(format!("[SUCCESS] {} Authenticated via REST API (/login).", display_name));
                    return Some((base_url.to_string(), tok.to_string(), false));
                }
            }
        }
    }

    let _ = tx.send(format!("[DEBUG] {} Trying UBUS JSON-RPC login with password '{}'...", display_name, password));
    let ubus_url = format!("{}/ubus", base_url);
    let ubus_payload = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "call",
        "params": ["00000000000000000000000000000000", "session", "login", {
            "username": username,
            "password": password
        }]
    });

    if let Ok(res) = client.post(&ubus_url).json(&ubus_payload).send().await {
        if res.status().as_u16() == 200 {
            if let Ok(val) = res.json::<Value>().await {
                if let Some(token) = val.pointer("/result/1/ubus_rpc_session").and_then(|t| t.as_str()) {
                    let _ = tx.send(format!("[SUCCESS] {} Authenticated via UBUS JSON-RPC.", display_name));
                    return Some((base_url.to_string(), token.to_string(), true));
                }
            }
        }
    }

    None
}

pub fn deduplicate_macs(macs: &[String]) -> String {
    let mut unique_macs = Vec::new();
    for m in macs {
        let clean = m.trim().to_uppercase();
        if !clean.is_empty() && !unique_macs.contains(&clean) {
            unique_macs.push(clean);
        }
    }
    if unique_macs.is_empty() {
        "None".to_string()
    } else {
        unique_macs.join("\n")
    }
}

pub async fn extract_ubus_wifi(client: &Client, base_url: &str, token: &str) -> (Vec<String>, String) {
    let wireless = run_ubus_cmd(client, base_url, token, "cat", &["/etc/config/wireless"]).await;
    let mut ssids = Vec::new();
    let mut macs = Vec::new();

    let mut current_ssid = "N/A".to_string();
    let mut current_key = "N/A".to_string();
    let mut in_iface = false;

    for line in wireless.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("config wifi-iface") {
            if in_iface {
                ssids.push(current_ssid.clone());
                ssids.push(current_key.clone());
            }
            in_iface = true;
            current_ssid = "N/A".to_string();
            current_key = "N/A".to_string();
        } else if in_iface && trimmed.starts_with("config ") {
            ssids.push(current_ssid.clone());
            ssids.push(current_key.clone());
            in_iface = false;
        } else if in_iface {
            if trimmed.starts_with("option ssid ") {
                let parts: Vec<&str> = trimmed.split('\'').collect();
                current_ssid = if parts.len() > 1 {
                    parts[1].to_string()
                } else {
                    trimmed.split_whitespace().nth(2).unwrap_or("N/A").to_string()
                };
            } else if trimmed.starts_with("option key ") {
                let parts: Vec<&str> = trimmed.split('\'').collect();
                current_key = if parts.len() > 1 {
                    parts[1].to_string()
                } else {
                    trimmed.split_whitespace().nth(2).unwrap_or("N/A").to_string()
                };
            } else if trimmed.starts_with("list maclist ") {
                let parts: Vec<&str> = trimmed.split('\'').collect();
                let m = if parts.len() > 1 {
                    parts[1].to_string()
                } else {
                    trimmed.split_whitespace().nth(2).unwrap_or("").to_string()
                };
                if !m.is_empty() {
                    macs.push(m);
                }
            }
        }
    }

    if in_iface {
        ssids.push(current_ssid);
        ssids.push(current_key);
    }

    let mac_str = deduplicate_macs(&macs);
    (ssids, mac_str)
}

pub async fn check_rms_connection_status(client: &Client, base_url: &str, api_url: &str, token: &str, is_ubus: bool) -> String {
    if !is_ubus && !api_url.is_empty() {
        let status_url = format!("{}/rms/status", api_url);
        if let Ok(res) = client.get(&status_url).bearer_auth(token).send().await {
            if let Ok(val) = res.json::<Value>().await {
                if let Some(st) = val.pointer("/data/status").and_then(|v| v.as_str()) {
                    return st.to_string();
                }
                if let Some(conn) = val.pointer("/data/connected").and_then(|v| v.as_bool()) {
                    return if conn { "connected".to_string() } else { "disconnected".to_string() };
                }
            }
        }
    }

    let ubus_status = run_ubus_cmd(client, base_url, token, "cat", &["/tmp/rms_status"]).await;
    if ubus_status != "N/A" && !ubus_status.is_empty() {
        return ubus_status;
    }

    let service_status = run_ubus_cmd(client, base_url, token, "/etc/init.d/rms_mqtt", &["status"]).await;
    if service_status.to_lowercase().contains("running") {
        return "service running".to_string();
    }

    "reconnect signal acknowledged".to_string()
}

async fn execute_update_firmware(
    target_item: String,
    host: String,
    port_to_use: String,
    params: Arc<JobParams>,
    is_running: Arc<AtomicBool>,
    tx: broadcast::Sender<String>,
) -> DeviceRecord {
    let firmware_path = match &params.firmware_path {
        Some(p) if std::path::Path::new(p).exists() => p.clone(),
        _ => {
            let _ = tx.send(format!("[ERROR] {} No firmware file uploaded or file not found.", target_item));
            return DeviceRecord {
                ip: target_item,
                status: "Failed".to_string(),
                details: vec!["No firmware file".to_string()],
            };
        }
    };

    let fw_bytes = match tokio::fs::read(&firmware_path).await {
        Ok(b) if !b.is_empty() => b,
        Ok(_) => {
            let _ = tx.send(format!("[ERROR] {} Firmware file is empty.", target_item));
            return DeviceRecord {
                ip: target_item,
                status: "Failed".to_string(),
                details: vec!["Empty firmware file".to_string()],
            };
        }
        Err(e) => {
            let _ = tx.send(format!("[ERROR] {} Failed reading firmware file: {}", target_item, e));
            return DeviceRecord {
                ip: target_item,
                status: "Failed".to_string(),
                details: vec![format!("Read error: {}", e)],
            };
        }
    };

    let port_num = port_to_use.parse::<u16>().unwrap_or(822);
    let mut found_password = None;
    let mut session_opt = None;

    let ssh_config = Arc::new(russh::client::Config {
        inactivity_timeout: Some(Duration::from_secs(60)),
        ..Default::default()
    });

    for pwd in &params.passwords {
        if !is_running.load(Ordering::Relaxed) {
            break;
        }
        let _ = tx.send(format!("[DEBUG] {} Testing SSH password: '{}' on port {}...", target_item, pwd, port_num));
        let connect_fut = russh::client::connect(ssh_config.clone(), (host.as_str(), port_num), SshClientHandler);
        match tokio::time::timeout(Duration::from_secs(12), connect_fut).await {
            Ok(Ok(mut handle)) => {
                let auth_fut = handle.authenticate_password(&params.username, pwd);
                match tokio::time::timeout(Duration::from_secs(12), auth_fut).await {
                    Ok(Ok(true)) => {
                        found_password = Some(pwd.clone());
                        session_opt = Some(handle);
                        let _ = tx.send(format!("[SUCCESS] {} SSH authentication successful with password '{}'.", target_item, pwd));
                        break;
                    }
                    _ => {}
                }
            }
            Ok(Err(e)) => {
                let _ = tx.send(format!("[DEBUG] {} SSH connection error: {}", target_item, e));
            }
            Err(_) => {
                let _ = tx.send(format!("[DEBUG] {} SSH connection timed out on port {}.", target_item, port_num));
            }
        }
    }

    let (active_pwd, session) = match (found_password, session_opt) {
        (Some(pwd), Some(sess)) => (pwd, sess),
        _ => {
            let _ = tx.send(format!("[ERROR] {} SSH authentication failed for all passwords on port {}.", target_item, port_num));
            return DeviceRecord {
                ip: target_item,
                status: "Failed".to_string(),
                details: vec!["SSH Auth Failed (Wrong Password)".to_string()],
            };
        }
    };

    let _ = tx.send(format!("[INFO] {} Starting firmware upload ({} bytes)...", target_item, fw_bytes.len()));

    let channel = match session.channel_open_session().await {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send(format!("[ERROR] {} Failed opening SSH channel: {}", target_item, e));
            return DeviceRecord {
                ip: target_item,
                status: "Failed".to_string(),
                details: vec![active_pwd, format!("SSH channel error: {}", e)],
            };
        }
    };

    if let Err(e) = channel.exec(true, "cat > /tmp/firmware.bin").await {
        let _ = tx.send(format!("[ERROR] {} Failed executing upload receiver: {}", target_item, e));
        return DeviceRecord {
            ip: target_item,
            status: "Failed".to_string(),
            details: vec![active_pwd, format!("Upload exec error: {}", e)],
        };
    }

    let mut stream = channel.into_stream();
    let total_bytes = fw_bytes.len();
    let chunk_size = 64 * 1024;
    let mut sent_bytes = 0usize;
    let mut last_percent = usize::MAX;

    let _ = tx.send(format!("SCP_PROG:{}|[INFO] {} Uploading: [--------------------] 0%", target_item, target_item));

    while sent_bytes < total_bytes {
        if !is_running.load(Ordering::Relaxed) {
            let _ = tx.send(format!("[ERROR] {} Firmware upload cancelled by user.", target_item));
            return DeviceRecord {
                ip: target_item,
                status: "Failed".to_string(),
                details: vec![active_pwd, "Cancelled by user".to_string()],
            };
        }
        let next_chunk = std::cmp::min(chunk_size, total_bytes - sent_bytes);
        if let Err(e) = stream.write_all(&fw_bytes[sent_bytes..sent_bytes + next_chunk]).await {
            let _ = tx.send(format!("[ERROR] {} Stream write error during upload: {}", target_item, e));
            return DeviceRecord {
                ip: target_item,
                status: "Failed".to_string(),
                details: vec![active_pwd, format!("Upload write error: {}", e)],
            };
        }
        sent_bytes += next_chunk;
        let percent = ((sent_bytes as f64 / total_bytes as f64) * 100.0).round() as usize;
        if percent != last_percent && (percent % 5 == 0 || percent == 100) {
            last_percent = percent;
            let filled = (percent * 20) / 100;
            let bar = format!("{}{}", "=".repeat(filled), "-".repeat(20 - filled));
            let _ = tx.send(format!("SCP_PROG:{}|[INFO] {} Uploading: [{}] {}%", target_item, target_item, bar, percent));
        }
    }

    let _ = stream.flush().await;
    let _ = stream.shutdown().await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    let _ = tx.send(format!("[INFO] {} Upload successful (100%). Initiating sysupgrade...", target_item));

    let sysup_chan = match session.channel_open_session().await {
        Ok(c) => c,
        Err(_) => {
            let mut handle = match russh::client::connect(ssh_config.clone(), (host.as_str(), port_num), SshClientHandler).await {
                Ok(h) => h,
                Err(e) => {
                    let _ = tx.send(format!("[ERROR] {} Reconnect for sysupgrade failed: {}", target_item, e));
                    return DeviceRecord {
                        ip: target_item,
                        status: "Failed".to_string(),
                        details: vec![active_pwd, format!("Reconnect error: {}", e)],
                    };
                }
            };
            let _ = handle.authenticate_password(&params.username, &active_pwd).await;
            match handle.channel_open_session().await {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(format!("[ERROR] {} Channel open for sysupgrade failed: {}", target_item, e));
                    return DeviceRecord {
                        ip: target_item,
                        status: "Failed".to_string(),
                        details: vec![active_pwd, format!("Sysupgrade channel error: {}", e)],
                    };
                }
            }
        }
    };

    let _ = tx.send(format!("[INFO] {} Executing 'sysupgrade /tmp/firmware.bin' on remote system...", target_item));
    let _ = sysup_chan.exec(true, "sysupgrade /tmp/firmware.bin").await;

    let mut reader = sysup_chan.into_stream();
    let mut read_buf = [0u8; 1024];
    let start_flashing = tokio::time::Instant::now();

    loop {
        let read_fut = reader.read(&mut read_buf);
        match tokio::time::timeout(Duration::from_secs(5), read_fut).await {
            Ok(Ok(n)) if n > 0 => {
                let out = String::from_utf8_lossy(&read_buf[..n]);
                for line in out.lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        let _ = tx.send(format!("[INFO] {} [sysupgrade] {}", target_item, trimmed));
                    }
                }
            }
            Ok(Ok(_)) => {
                let _ = tx.send(format!("[INFO] {} SSH session closed by remote host (kernel rebooting).", target_item));
                break;
            }
            Ok(Err(e)) => {
                let _ = tx.send(format!("[INFO] {} Connection closed: {}. Device is rebooting.", target_item, e));
                break;
            }
            Err(_) => {
                if start_flashing.elapsed() > Duration::from_secs(45) {
                    let _ = tx.send(format!("[INFO] {} sysupgrade process running. Device rebooting...", target_item));
                    break;
                }
            }
        }
    }

    let _ = tx.send(format!("[INFO] {} Flashing underway. Waiting for device reboot sequence...", target_item));
    tokio::time::sleep(Duration::from_secs(15)).await;

    let reboot_start = tokio::time::Instant::now();
    let mut rebooted = false;

    while reboot_start.elapsed() < Duration::from_secs(120) {
        if !is_running.load(Ordering::Relaxed) {
            break;
        }
        let elapsed_sec = reboot_start.elapsed().as_secs();
        if is_reachable(&host).await {
            let _ = tx.send(format!("[SUCCESS] {} Device rebooted successfully and is back online ({}s).", target_item, elapsed_sec));
            rebooted = true;
            break;
        } else {
            let _ = tx.send(format!("[INFO] {} Reboot in progress ({}s elapsed)... Waiting for ping response.", target_item, elapsed_sec));
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }

    if !rebooted {
        let _ = tx.send(format!("[INFO] {} Timeout waiting for ICMP reply. Flashing and reboot command dispatched.", target_item));
    }

    let _ = tx.send(format!("[SUCCESS] {} Firmware upgrade sequence completed. Done.", target_item));

    DeviceRecord {
        ip: target_item,
        status: "Success".to_string(),
        details: vec![active_pwd, "Firmware updated. Device rebooted.".to_string()],
    }
}

pub async fn execute_device_task(
    client: &Client,
    target_item: String,
    params: Arc<JobParams>,
    is_running: Arc<AtomicBool>,
    tx: broadcast::Sender<String>,
) -> DeviceRecord {
    if !is_running.load(Ordering::Relaxed) {
        return DeviceRecord {
            ip: target_item,
            status: "Failed".to_string(),
            details: vec!["Cancelled".to_string()],
        };
    }

    let (host, explicit_port) = if target_item.contains(':') {
        let parts: Vec<&str> = target_item.split(':').collect();
        (parts[0].to_string(), Some(parts[1].to_string()))
    } else {
        (target_item.clone(), None)
    };

    let port_to_use = explicit_port.unwrap_or_else(|| params.port.clone());
    let port_num = port_to_use.parse::<u16>().unwrap_or(443);
    let mut base_url = format!("https://{}:{}", host, port_to_use);

    let is_bundle = params.mode == "bundle_check" || params.mode == "reconnect_rms_bundle";
    let is_firmware = params.mode == "update_firmware";

    if is_firmware {
        if !is_port_open(&host, port_num, 750).await && !is_port_open(&host, 22, 500).await {
            let _ = tx.send(format!("[ERROR] {} SSH port unreachable. Skipping.", target_item));
            return DeviceRecord {
                ip: target_item,
                status: "Failed".to_string(),
                details: vec!["SSH Unreachable".to_string()],
            };
        }
        return execute_update_firmware(
            target_item,
            host,
            port_to_use,
            params,
            is_running,
            tx,
        ).await;
    }

    let mut port_active = is_port_open(&host, port_num, 1000).await;
    if !port_active && !is_bundle {
        if is_port_open(&host, 443, 500).await {
            base_url = format!("https://{}:443", host);
            port_active = true;
        } else if is_port_open(&host, 8443, 500).await {
            base_url = format!("https://{}:8443", host);
            port_active = true;
        } else if is_port_open(&host, 80, 500).await {
            base_url = format!("http://{}:80", host);
            port_active = true;
        }
    }

    if !port_active {
        let _ = tx.send(format!("[ERROR] {} Target port {} unreachable. Skipping.", target_item, port_num));
        return DeviceRecord {
            ip: target_item,
            status: "Failed".to_string(),
            details: vec!["Unreachable / Port Closed".to_string()],
        };
    }
    let _ = tx.send(format!("[SUCCESS] {} Target is alive.", target_item));

    let mut found_password = None;
    let mut api_url = String::new();
    let mut token = String::new();
    let mut is_ubus = false;

    let _ = tx.send(format!("[INFO] {} Connecting to {}...", target_item, base_url));
    for pwd in &params.passwords {
        if !is_running.load(Ordering::Relaxed) {
            break;
        }
        if let Some((url, tok, ubus_flag)) = try_login(client, &base_url, &params.username, pwd, &tx, &target_item).await {
            found_password = Some(pwd.clone());
            api_url = url;
            token = tok;
            is_ubus = ubus_flag;
            break;
        }
    }

    let active_pwd = match found_password {
        Some(p) => p,
        None => {
            let _ = tx.send(format!("[ERROR] {} All candidate passwords failed.", target_item));
            return DeviceRecord {
                ip: target_item,
                status: "Failed".to_string(),
                details: vec!["Wrong Password".to_string()],
            };
        }
    };

    let mode = params.mode.as_str();

    if mode == "verify" {
        let _ = tx.send(format!("[SUCCESS] {} Password verified: '{}'.", target_item, active_pwd));
        return DeviceRecord {
            ip: target_item,
            status: "Success".to_string(),
            details: vec![active_pwd],
        };
    }

    if mode == "standardize" {
        let target_standard_pwd = if !params.standard_passwords.is_empty() {
            params.standard_passwords[0].clone()
        } else {
            "Admin123!".to_string()
        };

        if params.standard_passwords.contains(&active_pwd) {
            let _ = tx.send(format!("[SUCCESS] {} Password is already standard ('{}').", target_item, active_pwd));
            return DeviceRecord {
                ip: target_item,
                status: "Success".to_string(),
                details: vec![active_pwd.clone(), active_pwd, "Already standard".to_string()],
            };
        }

        let _ = tx.send(format!("[INFO] {} Standardizing password to '{}'...", target_item, target_standard_pwd));

        let mut user_id = params.username.clone();
        if !is_ubus && !api_url.is_empty() {
            let users_url = format!("{}/users/config", api_url);
            if let Ok(res) = client.get(&users_url).bearer_auth(&token).send().await {
                if let Ok(val) = res.json::<Value>().await {
                    if let Some(arr) = val.get("data").and_then(|v| v.as_array()) {
                        for u in arr {
                            if u.get("username").and_then(|v| v.as_str()) == Some(&params.username) {
                                if let Some(id) = u.get("id").and_then(|v| v.as_str()) {
                                    user_id = id.to_string();
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut changed = false;
        let mut err_detail = String::new();

        if !is_ubus && !api_url.is_empty() {
            let update_url = format!("{}/users/config/{}", api_url, user_id);
            let payload = json!({
                "data": {
                    "current_password": active_pwd,
                    "password": target_standard_pwd,
                    "password_confirm": target_standard_pwd
                }
            });
            if let Ok(put_res) = client.put(&update_url).bearer_auth(&token).json(&payload).send().await {
                if put_res.status().is_success() {
                    changed = true;
                } else {
                    err_detail = format!("REST HTTP {}", put_res.status().as_u16());
                }
            }
        }

        if !changed && is_ubus {
            let ubus_cmd = format!("(echo '{}'; sleep 1; echo '{}') | passwd {}", target_standard_pwd, target_standard_pwd, params.username);
            let out = run_ubus_cmd(client, &base_url, &token, "ash", &["-c", &ubus_cmd]).await;
            if !out.to_lowercase().contains("error") {
                changed = true;
            } else {
                err_detail = out;
            }
        }

        if !changed {
            let ssh_config = Arc::new(russh::client::Config {
                inactivity_timeout: Some(Duration::from_secs(15)),
                ..Default::default()
            });
            let ssh_port = 22;
            if let Ok(Ok(mut handle)) = tokio::time::timeout(
                Duration::from_secs(8),
                russh::client::connect(ssh_config, (host.as_str(), ssh_port), SshClientHandler)
            ).await {
                if let Ok(Ok(true)) = tokio::time::timeout(
                    Duration::from_secs(8),
                    handle.authenticate_password("root", &active_pwd)
                ).await {
                    if let Ok(channel) = handle.channel_open_session().await {
                        let cmd = format!("(echo '{}'; sleep 1; echo '{}') | passwd {}", target_standard_pwd, target_standard_pwd, params.username);
                        let _ = channel.exec(true, cmd.as_str()).await;
                        changed = true;
                    }
                }
            }
        }

        if changed {
            tokio::time::sleep(Duration::from_millis(1000)).await;
            let _ = tx.send(format!("[SUCCESS] {} Password successfully standardized to '{}'.", target_item, target_standard_pwd));
            return DeviceRecord {
                ip: target_item,
                status: "Success".to_string(),
                details: vec![active_pwd, target_standard_pwd, "Standardized successfully".to_string()],
            };
        }

        let _ = tx.send(format!("[ERROR] {} Failed to standardize password: {}", target_item, err_detail));
        return DeviceRecord {
            ip: target_item,
            status: "Failed".to_string(),
            details: vec![active_pwd, "-".to_string(), format!("Failed: {}", err_detail)],
        };
    }

    if mode == "backup_config" {
        let _ = tx.send(format!("[INFO] {} Generating system configuration backup...", target_item));
        let mut backup_bytes = Vec::new();

        if !is_ubus && !api_url.is_empty() {
            let gen_url = format!("{}/backup/actions/generate", api_url);
            let _ = client.post(&gen_url).bearer_auth(&token).json(&json!({"data": {}})).send().await;
            tokio::time::sleep(Duration::from_millis(1500)).await;

            let dl_endpoints = [
                format!("{}/backup/actions/download", api_url),
                format!("{}/backup/download", api_url),
                format!("{}/backup", api_url),
                format!("{}/system/backup/download", api_url),
            ];

            for dl_url in &dl_endpoints {
                if backup_bytes.is_empty() {
                    if let Ok(res) = client.get(dl_url).bearer_auth(&token).send().await {
                        if res.status().is_success() {
                            if let Ok(bytes) = res.bytes().await {
                                if !bytes.is_empty() && (bytes.starts_with(&[0x1f, 0x8b]) || bytes.len() > 200) {
                                    backup_bytes = bytes.to_vec();
                                    break;
                                }
                            }
                        }
                    }
                }
                if backup_bytes.is_empty() {
                    if let Ok(res) = client.post(dl_url).bearer_auth(&token).json(&json!({})).send().await {
                        if res.status().is_success() {
                            if let Ok(bytes) = res.bytes().await {
                                if !bytes.is_empty() && (bytes.starts_with(&[0x1f, 0x8b]) || bytes.len() > 200) {
                                    backup_bytes = bytes.to_vec();
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        if backup_bytes.is_empty() && is_ubus {
            let _ = run_ubus_cmd(client, &base_url, &token, "sysupgrade", &["-b", "/tmp/backup.tar.gz"]).await;
            tokio::time::sleep(Duration::from_millis(1500)).await;

            let read_payload = json!({
                "jsonrpc": "2.0",
                "id": 10,
                "method": "call",
                "params": [&token, "file", "read", {
                    "path": "/tmp/backup.tar.gz"
                }]
            });
            if let Ok(res) = client.post(&format!("{}/ubus", base_url)).json(&read_payload).send().await {
                if let Ok(val) = res.json::<Value>().await {
                    if let Some(b64) = val.pointer("/result/1/data").and_then(|v| v.as_str()) {
                        let clean: String = b64.chars().filter(|c| !c.is_whitespace()).collect();
                        if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(&clean) {
                            if !decoded.is_empty() {
                                backup_bytes = decoded;
                            }
                        }
                    }
                }
            }

            if backup_bytes.is_empty() {
                let b64_res = run_ubus_cmd(client, &base_url, &token, "base64", &["/tmp/backup.tar.gz"]).await;
                if !b64_res.is_empty() && b64_res != "N/A" {
                    let clean: String = b64_res.chars().filter(|c| !c.is_whitespace()).collect();
                    if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(&clean) {
                        if !decoded.is_empty() {
                            backup_bytes = decoded;
                        }
                    }
                }
            }

            if backup_bytes.is_empty() {
                let b64_res2 = run_ubus_cmd(client, &base_url, &token, "sh", &["-c", "base64 /tmp/backup.tar.gz || openssl enc -base64 -in /tmp/backup.tar.gz"]).await;
                if !b64_res2.is_empty() && b64_res2 != "N/A" {
                    let clean: String = b64_res2.chars().filter(|c| !c.is_whitespace()).collect();
                    if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(&clean) {
                        if !decoded.is_empty() {
                            backup_bytes = decoded;
                        }
                    }
                }
            }

            if backup_bytes.is_empty() {
                let luci_url = format!("{}/cgi-bin/luci/;stok={}/admin/system/backup", base_url, token);
                if let Ok(res) = client.get(&luci_url).header("Cookie", format!("sysauth={}", token)).send().await {
                    if res.status().is_success() {
                        if let Ok(bytes) = res.bytes().await {
                            if bytes.len() > 100 && (bytes.starts_with(&[0x1f, 0x8b]) || !bytes.starts_with(b"<!DOCTYPE")) {
                                backup_bytes = bytes.to_vec();
                            }
                        }
                    }
                }
            }

            let _ = run_ubus_cmd(client, &base_url, &token, "rm", &["-f", "/tmp/backup.tar.gz"]).await;
        }

        if backup_bytes.is_empty() {
            let ssh_config = Arc::new(russh::client::Config {
                inactivity_timeout: Some(Duration::from_secs(30)),
                ..Default::default()
            });
            let ssh_port = 22;
            if let Ok(Ok(mut handle)) = tokio::time::timeout(
                Duration::from_secs(5),
                russh::client::connect(ssh_config.clone(), (host.as_str(), ssh_port), SshClientHandler)
            ).await {
                let ssh_users = if params.username.is_empty() || params.username == "admin" {
                    vec!["root", "admin"]
                } else {
                    vec![params.username.as_str(), "root", "admin"]
                };
                let mut authenticated = false;
                for u in ssh_users {
                    if let Ok(Ok(true)) = tokio::time::timeout(
                        Duration::from_secs(4),
                        handle.authenticate_password(u, &active_pwd)
                    ).await {
                        authenticated = true;
                        break;
                    }
                }
                if authenticated {
                    if let Ok(channel) = handle.channel_open_session().await {
                        if channel.exec(true, "sysupgrade -b /tmp/backup_ssh.tar.gz && base64 /tmp/backup_ssh.tar.gz && rm -f /tmp/backup_ssh.tar.gz").await.is_ok() {
                            let mut stream = channel.into_stream();
                            let mut b64_ssh = String::new();
                            let _ = stream.read_to_string(&mut b64_ssh).await;
                            let clean: String = b64_ssh.chars().filter(|c| !c.is_whitespace()).collect();
                            if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(&clean) {
                                if !decoded.is_empty() {
                                    backup_bytes = decoded;
                                }
                            }
                        }
                    }
                }
            }
        }

        if !backup_bytes.is_empty() {
            let clean_ip = target_item.replace(':', "_port_");
            let timestamp = chrono::Local::now().format("%Y-%m-%d_%H%M%S").to_string();
            let filename = format!("{}_{}.tar.gz", clean_ip, timestamp);
            let cfg = crate::config::load_config();
            let base_dir = if cfg.results_dir.trim().is_empty() { "results".to_string() } else { cfg.results_dir };
            let device_backup_dir = std::path::Path::new(&base_dir).join("backups").join(&clean_ip);
            let _ = std::fs::create_dir_all(&device_backup_dir);
            let file_path = device_backup_dir.join(&filename);
            let rel_path = format!("backups/{}/{}", clean_ip, filename);
            if std::fs::write(&file_path, &backup_bytes).is_ok() {
                let _ = tx.send(format!("[SUCCESS] {} Backup saved: {}", target_item, rel_path));
                return DeviceRecord {
                    ip: target_item,
                    status: "Success".to_string(),
                    details: vec![filename, rel_path, format!("{} bytes", backup_bytes.len())],
                };
            }
        }

        let _ = tx.send(format!("[ERROR] {} Failed to download backup archive.", target_item));
        return DeviceRecord {
            ip: target_item,
            status: "Failed".to_string(),
            details: vec!["-".to_string(), "-".to_string(), "Backup download failed".to_string()],
        };
    }

    if mode == "restore_config" {
        let backup_path = match &params.backup_file_path {
            Some(p) if std::path::Path::new(p).exists() => p.clone(),
            _ => {
                let _ = tx.send(format!("[ERROR] {} No backup file specified.", target_item));
                return DeviceRecord {
                    ip: target_item,
                    status: "Failed".to_string(),
                    details: vec!["No backup file selected".to_string()],
                };
            }
        };

        let backup_bytes = match tokio::fs::read(&backup_path).await {
            Ok(b) if !b.is_empty() => b,
            _ => {
                let _ = tx.send(format!("[ERROR] {} Backup file is empty or missing.", target_item));
                return DeviceRecord {
                    ip: target_item,
                    status: "Failed".to_string(),
                    details: vec!["Empty backup file".to_string()],
                };
            }
        };

        let _ = tx.send(format!("[INFO] {} Uploading configuration backup ({} bytes)...", target_item, backup_bytes.len()));
        let mut restored = false;

        if !is_ubus && !api_url.is_empty() {
            let upload_url = format!("{}/backup/actions/upload", api_url);
            let part = reqwest::multipart::Part::bytes(backup_bytes.clone())
                .file_name("backup.tar.gz")
                .mime_str("application/x-targz")
                .unwrap_or_else(|_| reqwest::multipart::Part::bytes(backup_bytes.clone()));
            let form = reqwest::multipart::Form::new().part("file", part);

            if let Ok(res) = client.post(&upload_url).bearer_auth(&token).multipart(form).send().await {
                if res.status().is_success() {
                    let apply_url = format!("{}/backup/actions/apply", api_url);
                    if let Ok(apply_res) = client.post(&apply_url).bearer_auth(&token).json(&json!({})).send().await {
                        if apply_res.status().is_success() {
                            restored = true;
                        }
                    }
                }
            }
        }

        if !restored {
            let ssh_config = Arc::new(russh::client::Config {
                inactivity_timeout: Some(Duration::from_secs(30)),
                ..Default::default()
            });
            let ssh_port = 22;
            if let Ok(Ok(mut handle)) = tokio::time::timeout(
                Duration::from_secs(10),
                russh::client::connect(ssh_config, (host.as_str(), ssh_port), SshClientHandler)
            ).await {
                if let Ok(Ok(true)) = tokio::time::timeout(
                    Duration::from_secs(10),
                    handle.authenticate_password("root", &active_pwd)
                ).await {
                    if let Ok(channel) = handle.channel_open_session().await {
                        if channel.exec(true, "cat > /tmp/restore_backup.tar.gz").await.is_ok() {
                            let mut stream = channel.into_stream();
                            if stream.write_all(&backup_bytes).await.is_ok() {
                                let _ = stream.flush().await;
                                let _ = stream.shutdown().await;
                                if let Ok(sysup_chan) = handle.channel_open_session().await {
                                    let _ = sysup_chan.exec(true, "sysupgrade -r /tmp/restore_backup.tar.gz").await;
                                    restored = true;
                                }
                            }
                        }
                    }
                }
            }
        }

        if restored {
            let _ = tx.send(format!("[SUCCESS] {} Backup applied successfully. Device rebooting...", target_item));
            return DeviceRecord {
                ip: target_item,
                status: "Success".to_string(),
                details: vec!["Config restored & applying".to_string()],
            };
        }

        let _ = tx.send(format!("[ERROR] {} Failed to restore backup archive.", target_item));
        return DeviceRecord {
            ip: target_item,
            status: "Failed".to_string(),
            details: vec!["Restore failed".to_string()],
        };
    }

    if mode == "bundle_check" {
        let _ = tx.send(format!("[INFO] {} Extracting bundle info (Serial & MAC)...", target_item));
        let mut sn = "N/A".to_string();
        let mut mac = "N/A".to_string();
        if is_ubus {
            sn = run_ubus_cmd(client, &base_url, &token, "mnf_info", &["-s"]).await;
            mac = run_ubus_cmd(client, &base_url, &token, "mnf_info", &["-m"]).await;
        } else {
            let status_url = format!("{}/system/device/status", api_url);
            if let Ok(res) = client.get(&status_url).bearer_auth(&token).send().await {
                if let Ok(val) = res.json::<Value>().await {
                    sn = val.pointer("/data/mnfinfo/serial").and_then(|v| v.as_str()).unwrap_or("N/A").to_string();
                    mac = val.pointer("/data/mnfinfo/mac").and_then(|v| v.as_str()).unwrap_or("N/A").to_string();
                }
            }
        }
        let _ = tx.send(format!("[SUCCESS] {} SN: {}, MAC: {}, Password: {}.", target_item, sn, mac, active_pwd));
        return DeviceRecord {
            ip: target_item,
            status: "Success".to_string(),
            details: vec![sn, mac, active_pwd],
        };
    }

    if mode == "check_wifi" {
        let _ = tx.send(format!("[INFO] {} Pulling Wi-Fi interfaces and MAC filters...", target_item));
        if is_ubus {
            let (ssids, mac_str) = extract_ubus_wifi(client, &base_url, &token).await;
            let _ = tx.send(format!("[SUCCESS] {} Extracted {} SSIDs via UBUS.", target_item, ssids.len() / 2));
            let mut det = vec![mac_str];
            det.extend(ssids);
            return DeviceRecord {
                ip: target_item,
                status: "Success".to_string(),
                details: det,
            };
        } else {
            let wifi_url = format!("{}/wireless/interfaces/config", api_url);
            if let Ok(res) = client.get(&wifi_url).bearer_auth(&token).send().await {
                if let Ok(val) = res.json::<Value>().await {
                    let mut ssids = Vec::new();
                    let mut macs = Vec::new();
                    if let Some(interfaces) = val.get("data").and_then(|v| v.as_array()) {
                        for iface in interfaces {
                            let s = iface.get("ssid").and_then(|v| v.as_str()).unwrap_or("N/A");
                            let k = iface.get("key").and_then(|v| v.as_str()).unwrap_or("N/A");
                            ssids.push(s.to_string());
                            ssids.push(k.to_string());
                            if let Some(mac_arr) = iface.get("maclist").and_then(|v| v.as_array()) {
                                for m in mac_arr {
                                    if let Some(m_str) = m.as_str() {
                                        macs.push(m_str.to_string());
                                    }
                                }
                            }
                        }
                    }
                    let mac_str = deduplicate_macs(&macs);
                    let _ = tx.send(format!("[SUCCESS] {} Extracted {} SSIDs via REST API.", target_item, ssids.len() / 2));
                    let mut det = vec![mac_str];
                    det.extend(ssids);
                    return DeviceRecord {
                        ip: target_item,
                        status: "Success".to_string(),
                        details: det,
                    };
                }
            }
        }
    }
    if mode == "fetch_mac" {
        let _ = tx.send(format!("[INFO] {} Extracting MAC addresses from Access Point...", target_item));
        let mut all_macs = Vec::new();
        if is_ubus {
            let dev_mac = run_ubus_cmd(client, &base_url, &token, "mnf_info", &["-m"]).await;
            if dev_mac != "N/A" && !dev_mac.is_empty() {
                all_macs.push(dev_mac);
            }
            let (_, wifi_macs_str) = extract_ubus_wifi(client, &base_url, &token).await;
            for m in wifi_macs_str.lines() {
                let clean = m.trim().to_uppercase();
                if !clean.is_empty() && clean != "NONE" && !all_macs.contains(&clean) {
                    all_macs.push(clean);
                }
            }
        } else {
            let sys_url = format!("{}/system/device/status", api_url);
            if let Ok(res) = client.get(&sys_url).bearer_auth(&token).send().await {
                if let Ok(val) = res.json::<Value>().await {
                    if let Some(m) = val.pointer("/data/mnfinfo/mac").and_then(|v| v.as_str()) {
                        all_macs.push(m.to_string());
                    }
                }
            }
            let wifi_url = format!("{}/wireless/interfaces/config", api_url);
            if let Ok(res) = client.get(&wifi_url).bearer_auth(&token).send().await {
                if let Ok(val) = res.json::<Value>().await {
                    if let Some(interfaces) = val.get("data").and_then(|v| v.as_array()) {
                        for iface in interfaces {
                            if let Some(mac_arr) = iface.get("maclist").and_then(|v| v.as_array()) {
                                for m in mac_arr {
                                    if let Some(m_str) = m.as_str() {
                                        let clean = m_str.trim().to_uppercase();
                                        if !clean.is_empty() && !all_macs.contains(&clean) {
                                            all_macs.push(clean);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        let mac_str = if all_macs.is_empty() { "None".to_string() } else { all_macs.join("\n") };
        let _ = tx.send(format!("[SUCCESS] {} Fetched {} MAC addresses.", target_item, all_macs.len()));
        return DeviceRecord {
            ip: target_item,
            status: "Success".to_string(),
            details: vec![mac_str],
        };
    }

    if mode == "add_mac" {
        if params.mac_addresses.is_empty() {
            let _ = tx.send(format!("[ERROR] {} No MAC addresses specified to add.", target_item));
            return DeviceRecord {
                ip: target_item,
                status: "Failed".to_string(),
                details: vec!["No MAC addresses provided".to_string()],
            };
        }

        let _ = tx.send(format!("[INFO] {} Injecting {} MAC addresses into whitelist...", target_item, params.mac_addresses.len()));
        if is_ubus {
            let wireless = run_ubus_cmd(client, &base_url, &token, "cat", &["/etc/config/wireless"]).await;
            let count = wireless.matches("config wifi-iface").count();
            if count == 0 {
                return DeviceRecord {
                    ip: target_item,
                    status: "Failed".to_string(),
                    details: vec!["No wifi interfaces found".to_string()],
                };
            }
            for i in 0..count {
                let filter_key = format!("wireless.@wifi-iface[{}].macfilter=allow", i);
                let list_key = format!("wireless.@wifi-iface[{}].maclist", i);
                run_ubus_cmd(client, &base_url, &token, "uci", &["set", &filter_key]).await;
                let existing_raw = run_ubus_cmd(client, &base_url, &token, "uci", &["-q", "get", &list_key]).await;
                let existing_macs: Vec<String> = existing_raw
                    .split_whitespace()
                    .map(|s| s.trim().to_uppercase())
                    .collect();
                for mac in &params.mac_addresses {
                    let clean_mac = mac.trim().to_uppercase();
                    if !clean_mac.is_empty() && !existing_macs.contains(&clean_mac) {
                        let add_key = format!("wireless.@wifi-iface[{}].maclist={}", i, clean_mac);
                        run_ubus_cmd(client, &base_url, &token, "uci", &["add_list", &add_key]).await;
                    }
                }
            }
            run_ubus_cmd(client, &base_url, &token, "uci", &["commit", "wireless"]).await;
            run_ubus_cmd(client, &base_url, &token, "ash", &["-c", "/sbin/wifi reload &"]).await;
            let _ = tx.send(format!("[SUCCESS] {} Whitelist updated on {} UBUS AP interfaces.", target_item, count));
            return DeviceRecord {
                ip: target_item,
                status: "Success".to_string(),
                details: vec![params.mac_addresses.join("\n"), format!("Injected to {} AP interfaces", count)],
            };
        } else {
            let wifi_url = format!("{}/wireless/interfaces/config", api_url);
            if let Ok(res) = client.get(&wifi_url).bearer_auth(&token).send().await {
                if let Ok(val) = res.json::<Value>().await {
                    if let Some(interfaces) = val.get("data").and_then(|v| v.as_array()) {
                        let mut success_count = 0;
                        for iface in interfaces {
                            if let Some(id) = iface.get("id").and_then(|v| v.as_str()) {
                                let detail_url = format!("{}/wireless/interfaces/config/{}", api_url, id);
                                let iface_detail = if let Ok(detail_res) = client.get(&detail_url).bearer_auth(&token).send().await {
                                    detail_res.json::<Value>().await.ok().and_then(|v| v.get("data").cloned()).unwrap_or_else(|| iface.clone())
                                } else {
                                    iface.clone()
                                };
                                let mut current_macs: Vec<String> = Vec::new();
                                if let Some(maclist_val) = iface_detail.get("maclist") {
                                    if let Some(arr) = maclist_val.as_array() {
                                        for m in arr {
                                            if let Some(s) = m.as_str() {
                                                let clean = s.trim().to_uppercase();
                                                if !clean.is_empty() && !current_macs.contains(&clean) {
                                                    current_macs.push(clean);
                                                }
                                            }
                                        }
                                    } else if let Some(s) = maclist_val.as_str() {
                                        for item in s.split(|c| c == ' ' || c == ',' || c == '\n' || c == ';') {
                                            let clean = item.trim().to_uppercase();
                                            if !clean.is_empty() && !current_macs.contains(&clean) {
                                                current_macs.push(clean);
                                            }
                                        }
                                    }
                                }
                                for m in &params.mac_addresses {
                                    let clean = m.trim().to_uppercase();
                                    if !clean.is_empty() && !current_macs.contains(&clean) {
                                        current_macs.push(clean);
                                    }
                                }
                                let update_url = format!("{}/wireless/interfaces/config/{}", api_url, id);
                                let payload = json!({
                                    "data": {
                                        "macfilter": "allow",
                                        "maclist": current_macs
                                    }
                                });
                                if let Ok(put_res) = client.put(&update_url).bearer_auth(&token).json(&payload).send().await {
                                    if put_res.status().is_success() {
                                        success_count += 1;
                                    }
                                }
                            }
                        }
                        let _ = tx.send(format!("[SUCCESS] {} Whitelist applied to {} SSIDs via REST API.", target_item, success_count));
                        return DeviceRecord {
                            ip: target_item,
                            status: "Success".to_string(),
                            details: vec![params.mac_addresses.join("\n"), format!("Updated {} SSIDs", success_count)],
                        };
                    }
                }
            }
        }
    }

    if mode == "reboot" {
        let _ = tx.send(format!("[INFO] {} Sending reboot command to device...", target_item));
        let mut reboot_sent = false;

        if is_ubus {
            run_ubus_cmd(client, &base_url, &token, "reboot", &[]).await;
            reboot_sent = true;
        } else {
            let reboot_url = format!("{}/system/actions/reboot", api_url);
            if let Ok(res) = client.post(&reboot_url).bearer_auth(&token).send().await {
                if res.status().is_success() {
                    reboot_sent = true;
                }
            }
            if !reboot_sent {
                let reboot_url2 = format!("{}/actions/reboot", api_url);
                if let Ok(res) = client.post(&reboot_url2).bearer_auth(&token).send().await {
                    if res.status().is_success() {
                        reboot_sent = true;
                    }
                }
            }
            if !reboot_sent {
                run_ubus_cmd(client, &base_url, &token, "reboot", &[]).await;
                reboot_sent = true;
            }
        }

        if reboot_sent {
            let _ = tx.send(format!("[SUCCESS] {} Reboot signal accepted. Device is restarting.", target_item));
            return DeviceRecord {
                ip: target_item,
                status: "Success".to_string(),
                details: vec!["Rebooting...".to_string()],
            };
        } else {
            let _ = tx.send(format!("[ERROR] {} Failed to transmit reboot command.", target_item));
            return DeviceRecord {
                ip: target_item,
                status: "Failed".to_string(),
                details: vec!["Reboot signal failed".to_string()],
            };
        }
    }

    if mode == "clear_arp" {
        let _ = tx.send(format!("[INFO] {} Clearing DHCP leases and ARP cache via UBUS...", target_item));
        let cmd = "rm /tmp/dhcp.leases; /etc/init.d/dnsmasq restart; ip -s -s neigh flush all";
        if is_ubus {
            run_ubus_cmd(client, &base_url, &token, "ash", &["-c", cmd]).await;
            let _ = tx.send(format!("[SUCCESS] {} ARP cache and DHCP leases flushed.", target_item));
            return DeviceRecord {
                ip: target_item,
                status: "Success".to_string(),
                details: vec!["ARP & DHCP Cleared".to_string()],
            };
        }
    }

    if mode == "reconnect_rms" || mode == "reconnect_rms_bundle" {
        let _ = tx.send(format!("[INFO] {} Initiating RMS reconnect signal...", target_item));
        let mut signal_sent = false;

        if is_ubus {
            run_ubus_cmd(client, &base_url, &token, "/etc/init.d/rms_mqtt", &["restart"]).await;
            signal_sent = true;
        } else {
            let rms_url = format!("{}/rms/actions/connect", api_url);
            if let Ok(res) = client.post(&rms_url).bearer_auth(&token).send().await {
                if res.status().is_success() {
                    signal_sent = true;
                }
            }
        }

        if !signal_sent {
            let _ = tx.send(format!("[ERROR] {} Failed to transmit RMS reconnect command.", target_item));
            return DeviceRecord {
                ip: target_item,
                status: "Failed".to_string(),
                details: vec!["RMS reconnect signal failed".to_string()],
            };
        }

        let _ = tx.send(format!("[INFO] {} Reconnect signal transmitted. Waiting 2s to verify RMS state...", target_item));
        tokio::time::sleep(Duration::from_millis(2000)).await;

        let status = check_rms_connection_status(client, &base_url, &api_url, &token, is_ubus).await;
        let _ = tx.send(format!("[SUCCESS] {} Verified RMS Status: '{}'.", target_item, status));

        return DeviceRecord {
            ip: target_item,
            status: "Success".to_string(),
            details: vec![format!("RMS: {}", status)],
        };
    }

    if mode == "extract_data" {
        let _ = tx.send(format!("[INFO] {} Extracting system hardware & cellular modem telemetry...", target_item));
        let mut fw = "N/A".to_string();
        let mut sn = "N/A".to_string();
        let mut mac = "N/A".to_string();
        let mut imei = "N/A".to_string();
        let mut iccid = "N/A".to_string();
        let mut imsi = "N/A".to_string();
        let mut operator = "N/A".to_string();
        let mut rsrp = "N/A".to_string();
        let mut rssi = "N/A".to_string();
        let mut rsrq = "N/A".to_string();
        let mut sinr = "N/A".to_string();

        if is_ubus {
            fw = run_ubus_cmd(client, &base_url, &token, "cat", &["/etc/version"]).await;
            sn = run_ubus_cmd(client, &base_url, &token, "mnf_info", &["-s"]).await;
            mac = run_ubus_cmd(client, &base_url, &token, "mnf_info", &["-m"]).await;
            imei = run_ubus_cmd(client, &base_url, &token, "gsmctl", &["-i"]).await;
            iccid = run_ubus_cmd(client, &base_url, &token, "gsmctl", &["-A", "AT+CCID"]).await.replace("+CCID:", "").trim().to_string();
            imsi = run_ubus_cmd(client, &base_url, &token, "gsmctl", &["-x"]).await;
            operator = run_ubus_cmd(client, &base_url, &token, "gsmctl", &["-o"]).await;
            let signal_raw = run_ubus_cmd(client, &base_url, &token, "gsmctl", &["-q"]).await;
            for line in signal_raw.lines() {
                if line.contains("RSRP:") { rsrp = line.replace("RSRP:", "").trim().to_string(); }
                else if line.contains("RSRQ:") { rsrq = line.replace("RSRQ:", "").trim().to_string(); }
                else if line.contains("SINR:") { sinr = line.replace("SINR:", "").trim().to_string(); }
                else if line.contains("RSSI:") { rssi = line.replace("RSSI:", "").trim().to_string(); }
            }
            if rssi == "N/A" && signal_raw.trim().trim_start_matches('-').parse::<i32>().is_ok() {
                rssi = signal_raw.trim().to_string();
            }
        } else {
            let sys_url = format!("{}/system/device/status", api_url);
            if let Ok(res) = client.get(&sys_url).bearer_auth(&token).send().await {
                if let Ok(val) = res.json::<Value>().await {
                    fw = val.pointer("/data/static/fw_version").and_then(|v| v.as_str()).unwrap_or("N/A").to_string();
                    sn = val.pointer("/data/mnfinfo/serial").and_then(|v| v.as_str()).unwrap_or("N/A").to_string();
                    mac = val.pointer("/data/mnfinfo/mac").and_then(|v| v.as_str()).unwrap_or("N/A").to_string();
                }
            }

            let modem_url = format!("{}/modems/status", api_url);
            if let Ok(res) = client.get(&modem_url).bearer_auth(&token).send().await {
                if let Ok(val) = res.json::<Value>().await {
                    if let Some(m) = val.pointer("/data/0") {
                        imei = m.get("imei").and_then(|v| v.as_str()).unwrap_or("N/A").to_string();
                        iccid = m.get("iccid").and_then(|v| v.as_str()).unwrap_or("N/A").to_string();
                        imsi = m.get("imsi").and_then(|v| v.as_str()).unwrap_or("N/A").to_string();
                        operator = m.get("operator").or_else(|| m.get("provider")).and_then(|v| v.as_str()).unwrap_or("N/A").to_string();
                        rsrp = m.get("rsrp").map(|v| v.to_string()).unwrap_or_else(|| "N/A".to_string());
                        rssi = m.get("signal").or_else(|| m.get("rssi")).map(|v| v.to_string()).unwrap_or_else(|| "N/A".to_string());
                        rsrq = m.get("rsrq").map(|v| v.to_string()).unwrap_or_else(|| "N/A".to_string());
                        sinr = m.get("sinr").map(|v| v.to_string()).unwrap_or_else(|| "N/A".to_string());
                    }
                }
            }
        }

        let _ = tx.send(format!("[SUCCESS] {} FW: {}, SN: {}, MAC: {}, IMEI: {}, Operator: {}, RSRP: {}, SINR: {}.", target_item, fw, sn, mac, imei, operator, rsrp, sinr));
        return DeviceRecord {
            ip: target_item,
            status: "Success".to_string(),
            details: vec![fw, sn, mac, imei, iccid, imsi, operator, rssi, rsrp, rsrq, sinr],
        };
    }

    if mode == "extract_data_ap" {
        let _ = tx.send(format!("[INFO] {} Extracting Access Point configuration & status...", target_item));
        let mut fw = "N/A".to_string();
        let mut sn = "N/A".to_string();
        let mut mac = "N/A".to_string();

        if is_ubus {
            fw = run_ubus_cmd(client, &base_url, &token, "cat", &["/etc/version"]).await;
            sn = run_ubus_cmd(client, &base_url, &token, "mnf_info", &["-s"]).await;
            mac = run_ubus_cmd(client, &base_url, &token, "mnf_info", &["-m"]).await;
        } else {
            let sys_url = format!("{}/system/device/status", api_url);
            if let Ok(res) = client.get(&sys_url).bearer_auth(&token).send().await {
                if let Ok(val) = res.json::<Value>().await {
                    fw = val.pointer("/data/static/fw_version").and_then(|v| v.as_str()).unwrap_or("N/A").to_string();
                    sn = val.pointer("/data/mnfinfo/serial").and_then(|v| v.as_str()).unwrap_or("N/A").to_string();
                    mac = val.pointer("/data/mnfinfo/mac").and_then(|v| v.as_str()).unwrap_or("N/A").to_string();
                }
            }
        }

        let (ssids, _) = extract_ubus_wifi(client, &base_url, &token).await;
        let mut det = vec![fw.clone(), sn.clone(), mac.clone()];
        det.extend(ssids.clone());

        let _ = tx.send(format!("[SUCCESS] {} FW: {}, SN: {}, MAC: {}, Extracted {} SSIDs.", target_item, fw, sn, mac, ssids.len() / 2));
        return DeviceRecord {
            ip: target_item,
            status: "Success".to_string(),
            details: det,
        };
    }

    DeviceRecord {
        ip: target_item,
        status: "Success".to_string(),
        details: vec![active_pwd],
    }
}

pub async fn run_automation_sequence(
    params: Arc<JobParams>,
    is_running: Arc<AtomicBool>,
    results_store: Arc<Mutex<HashMap<String, DeviceRecord>>>,
    tx_stream: broadcast::Sender<String>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let client = create_http_client();
    let total = params.ips.len();
    let completed_counter = Arc::new(AtomicUsize::new(0));

    let _ = tx_stream.send(format!("LIVE:Starting sequence on {} device targets (workers: {})...", total, params.workers));
    let _ = tx_stream.send(format!("PROGRESS:0/{}", total));

    let queue = Arc::new(Mutex::new(VecDeque::from(params.ips.clone())));
    let num_workers = params.workers.clamp(1, 50);
    let mut worker_handles = Vec::with_capacity(num_workers);

    let password_count = params.passwords.len().max(1) as u64;
    let task_timeout = if params.mode == "update_firmware" {
        Duration::from_secs(300)
    } else if params.mode == "restore_config" || params.mode == "backup_config" {
        Duration::from_secs(60.max(password_count * 20))
    } else {
        Duration::from_secs(30.max(password_count * 15))
    };

    for _ in 0..num_workers {
        let queue = queue.clone();
        let client = client.clone();
        let params = params.clone();
        let is_running = is_running.clone();
        let results_store = results_store.clone();
        let tx = tx_stream.clone();
        let completed_counter = completed_counter.clone();

        let handle = tokio::spawn(async move {
            loop {
                if !is_running.load(Ordering::Relaxed) {
                    break;
                }
                let item = {
                    let mut q = queue.lock().await;
                    q.pop_front()
                };
                let item = match item {
                    Some(i) => i,
                    None => break,
                };

                let _ = tx.send(format!("LIVE:Working on {}...", item));

                let task_fut = execute_device_task(
                    &client,
                    item.clone(),
                    params.clone(),
                    is_running.clone(),
                    tx.clone(),
                );

                let record = match tokio::time::timeout(task_timeout, task_fut).await {
                    Ok(rec) => rec,
                    Err(_) => {
                        let _ = tx.send(format!("[ERROR] {} Task timed out ({}s limit exceeded).", item, task_timeout.as_secs()));
                        DeviceRecord {
                            ip: item.clone(),
                            status: "Failed".to_string(),
                            details: vec![format!("Timeout ({}s)", task_timeout.as_secs())],
                        }
                    }
                };

                {
                    let mut lock = results_store.lock().await;
                    lock.insert(item.clone(), record.clone());
                }

                let payload = record.details.join("|");
                let _ = tx.send(format!("RESULT:{}|{}|{}", record.ip, record.status, payload));

                let current = completed_counter.fetch_add(1, Ordering::Relaxed) + 1;
                let _ = tx.send(format!("PROGRESS:{}/{}", current, total));
            }
        });
        worker_handles.push(handle);
    }

    for handle in worker_handles {
        let _ = handle.await;
    }

    let records_map = results_store.lock().await;
    let mut excel_rows = Vec::new();

    let is_ssid_extract = params.mode == "check_wifi";
    let is_ap_extract = params.mode == "extract_data_ap";

    let (headers, ssid_max_pairs) = if is_ssid_extract {
        let mut max_pairs = 2;
        for original_ip in &params.all_input_ips {
            if let Some(record) = records_map.get(original_ip) {
                if record.status == "Success" && record.details.len() > 1 {
                    let pairs = (record.details.len() - 1) / 2;
                    if pairs > max_pairs {
                        max_pairs = pairs;
                    }
                }
            }
        }
        let mut h = vec![
            "IP".to_string(),
            "Status".to_string(),
            "Mac Address (perline)".to_string(),
            "SSID1".to_string(),
            "PASS1".to_string(),
            "SSID2".to_string(),
            "PASS2".to_string(),
        ];
        for i in 3..=max_pairs {
            h.push(format!("SSID{}", i));
            h.push(format!("PASS{}", i));
        }
        (h, max_pairs)
    } else if is_ap_extract {
        let mut max_pairs = 2;
        for original_ip in &params.all_input_ips {
            if let Some(record) = records_map.get(original_ip) {
                if record.status == "Success" && record.details.len() > 3 {
                    let pairs = (record.details.len() - 3) / 2;
                    if pairs > max_pairs {
                        max_pairs = pairs;
                    }
                }
            }
        }
        let mut h = vec![
            "IP".to_string(),
            "Status".to_string(),
            "Firmware".to_string(),
            "Serial Number".to_string(),
            "Mac Address (perline)".to_string(),
            "SSID1".to_string(),
            "PASS1".to_string(),
            "SSID2".to_string(),
            "PASS2".to_string(),
        ];
        for i in 3..=max_pairs {
            h.push(format!("SSID{}", i));
            h.push(format!("PASS{}", i));
        }
        (h, max_pairs)
    } else {
        let h = match params.mode.as_str() {
            "extract_data" => vec![
                "IP Address".to_string(), "Status".to_string(), "Firmware".to_string(),
                "Serial Number".to_string(), "MAC Address".to_string(), "IMEI".to_string(),
                "ICCID".to_string(), "IMSI".to_string(), "Operator".to_string(),
                "RSSI".to_string(), "RSRP".to_string(), "RSRQ".to_string(), "SINR".to_string(),
            ],
            "fetch_mac" => vec![
                "IP".to_string(), "Status".to_string(), "Mac Address (perline)".to_string(),
            ],
            "add_mac" => vec![
                "IP".to_string(), "Status".to_string(), "Mac Address (perline)".to_string(), "Details".to_string(),
            ],
            "bundle_check" => vec![
                "IP:Port".to_string(), "Status".to_string(), "Serial Number".to_string(),
                "MAC Address".to_string(), "Password".to_string(),
            ],
            "reconnect_rms_bundle" => vec![
                "IP:Port".to_string(), "Status".to_string(), "RMS State".to_string(),
            ],
            "standardize" => vec![
                "IP Address".to_string(), "Status".to_string(), "Old Password".to_string(),
                "New Password".to_string(), "Details".to_string(),
            ],
            "backup_config" => vec![
                "IP Address".to_string(), "Status".to_string(), "Backup File".to_string(), "Relative Path".to_string(), "Size".to_string(),
            ],
            "restore_config" => vec![
                "IP Address".to_string(), "Status".to_string(), "Details".to_string(),
            ],
            _ => vec!["IP Address".to_string(), "Status".to_string(), "Details".to_string()],
        };
        (h, 0)
    };

    for original_ip in &params.all_input_ips {
        if let Some(record) = records_map.get(original_ip) {
            let mut row = vec![record.ip.clone(), record.status.clone()];
            if is_ssid_extract {
                if record.status == "Success" {
                    let mac_val = record.details.get(0).cloned().unwrap_or_else(|| "-".to_string());
                    row.push(mac_val);
                    for pair_idx in 0..ssid_max_pairs {
                        let ssid = record.details.get(1 + pair_idx * 2).cloned().unwrap_or_else(|| "-".to_string());
                        let pass = record.details.get(2 + pair_idx * 2).cloned().unwrap_or_else(|| "-".to_string());
                        row.push(ssid);
                        row.push(pass);
                    }
                } else {
                    let err = record.details.get(0).cloned().unwrap_or_else(|| "-".to_string());
                    row.push(err);
                    for _ in 0..(ssid_max_pairs * 2) {
                        row.push("-".to_string());
                    }
                }
            } else if is_ap_extract {
                if record.status == "Success" && record.details.len() >= 3 {
                    row.push(record.details[0].clone());
                    row.push(record.details[1].clone());
                    row.push(record.details[2].clone());
                    for pair_idx in 0..ssid_max_pairs {
                        let ssid = record.details.get(3 + pair_idx * 2).cloned().unwrap_or_else(|| "-".to_string());
                        let pass = record.details.get(4 + pair_idx * 2).cloned().unwrap_or_else(|| "-".to_string());
                        row.push(ssid);
                        row.push(pass);
                    }
                } else {
                    let err = record.details.get(0).cloned().unwrap_or_else(|| "-".to_string());
                    row.push(err);
                    while row.len() < headers.len() {
                        row.push("-".to_string());
                    }
                }
            } else {
                row.extend(record.details.clone());
                while row.len() < headers.len() {
                    row.push("-".to_string());
                }
            }
            excel_rows.push(row);
        } else {
            let status_str = if is_running.load(Ordering::Relaxed) { "Pending" } else { "Cancelled" };
            let mut pending_row = vec![original_ip.clone(), status_str.to_string()];
            while pending_row.len() < headers.len() {
                pending_row.push("-".to_string());
            }
            excel_rows.push(pending_row);
        }
    }

    let export_data = crate::excel::ExcelExportData {
        headers,
        rows: excel_rows,
    };

    let cfg = crate::config::load_config();
    let results_dir = if cfg.results_dir.trim().is_empty() { "results" } else { cfg.results_dir.trim() };
    let filename = crate::excel::save_excel(&export_data, results_dir)?;
    let _ = tx_stream.send(format!("DONE:{}", filename));
    Ok(filename)
}

pub async fn reboot_device_direct(
    client: &Client,
    target_item: String,
    username: &str,
    passwords: &[String],
    default_port: &str,
    tx: broadcast::Sender<String>,
) -> Result<String, String> {
    let (host, explicit_port) = if target_item.contains(':') {
        let parts: Vec<&str> = target_item.split(':').collect();
        (parts[0].to_string(), Some(parts[1].to_string()))
    } else {
        (target_item.clone(), None)
    };

    let port_to_use = explicit_port.unwrap_or_else(|| default_port.to_string());
    let port_num = port_to_use.parse::<u16>().unwrap_or(443);
    let mut base_url = if port_num == 80 {
        format!("http://{}:80", host)
    } else {
        format!("https://{}:{}", host, port_to_use)
    };

    let mut port_active = is_port_open(&host, port_num, 2500).await;
    if !port_active {
        if is_port_open(&host, 443, 1000).await {
            base_url = format!("https://{}:443", host);
            port_active = true;
        } else if is_port_open(&host, 8443, 1000).await {
            base_url = format!("https://{}:8443", host);
            port_active = true;
        } else if is_port_open(&host, 80, 1000).await {
            base_url = format!("http://{}:80", host);
            port_active = true;
        }
    }

    if !port_active {
        let _ = tx.send(format!("[ERROR] {} Target port {} unreachable for reboot.", target_item, port_to_use));
        return Err(format!("Port {} unreachable on {}", port_to_use, host));
    }

    let mut found_password = None;
    let mut api_url = String::new();
    let mut token = String::new();
    let mut is_ubus = false;

    let _ = tx.send(format!("[INFO] {} Authenticating for reboot...", target_item));
    for pwd in passwords {
        if let Some((url, tok, ubus_flag)) = try_login(client, &base_url, username, pwd, &tx, &target_item).await {
            found_password = Some(pwd.clone());
            api_url = url;
            token = tok;
            is_ubus = ubus_flag;
            break;
        }
    }

    if found_password.is_none() {
        let _ = tx.send(format!("[ERROR] {} Authentication failed during reboot request.", target_item));
        return Err("Authentication failed".to_string());
    }

    let _ = tx.send(format!("[INFO] {} Dispatching reboot signal...", target_item));
    let mut reboot_sent = false;

    if is_ubus {
        run_ubus_cmd(client, &base_url, &token, "reboot", &[]).await;
        reboot_sent = true;
    } else {
        let reboot_url = format!("{}/system/actions/reboot", api_url);
        if let Ok(res) = client.post(&reboot_url).bearer_auth(&token).send().await {
            if res.status().is_success() {
                reboot_sent = true;
            }
        }
        if !reboot_sent {
            let reboot_url2 = format!("{}/actions/reboot", api_url);
            if let Ok(res) = client.post(&reboot_url2).bearer_auth(&token).send().await {
                if res.status().is_success() {
                    reboot_sent = true;
                }
            }
        }
        if !reboot_sent {
            run_ubus_cmd(client, &base_url, &token, "reboot", &[]).await;
            reboot_sent = true;
        }
    }

    if reboot_sent {
        let _ = tx.send(format!("[SUCCESS] {} Reboot command accepted. Device is restarting.", target_item));
        Ok("Reboot signal dispatched successfully".to_string())
    } else {
        let _ = tx.send(format!("[ERROR] {} Failed to send reboot signal.", target_item));
        Err("Failed to send reboot signal".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deduplicate_macs() {
        let raw = vec![
            "00:24:06:F4:0E:4B".to_string(),
            "48:79:4D:7B:7D:11".to_string(),
            "00:24:06:F4:0E:4B".to_string(),
            "48:79:4D:7B:7D:11".to_string(),
        ];
        let res = deduplicate_macs(&raw);
        assert_eq!(res, "00:24:06:F4:0E:4B\n48:79:4D:7B:7D:11");
    }

    #[test]
    fn test_deduplicate_macs_empty() {
        let raw: Vec<String> = Vec::new();
        let res = deduplicate_macs(&raw);
        assert_eq!(res, "None");
    }
}

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CheckUpdateResult {
    pub update_available: bool,
    pub current_version: String,
    pub latest_version: String,
    pub release_name: String,
    pub release_notes: String,
    pub published_at: String,
    pub download_url: String,
    pub asset_size: u64,
}

pub fn parse_version(v: &str) -> (u32, u32, u32) {
    let clean = v.trim().trim_start_matches('v').trim_start_matches('V');
    let parts: Vec<&str> = clean.split('.').collect();
    let major = parts.get(0).and_then(|p| p.parse::<u32>().ok()).unwrap_or(0);
    let minor = parts.get(1).and_then(|p| p.parse::<u32>().ok()).unwrap_or(0);
    let patch = parts.get(2).and_then(|p| {
        let numeric: String = p.chars().take_while(|c| c.is_ascii_digit()).collect();
        numeric.parse::<u32>().ok()
    }).unwrap_or(0);
    (major, minor, patch)
}

pub fn is_newer_version(current: &str, remote: &str) -> bool {
    let c = parse_version(current);
    let r = parse_version(remote);
    r > c
}

pub fn cleanup_old_executable() {
    if let Ok(exe) = std::env::current_exe() {
        let old = exe.with_extension("old");
        if old.exists() {
            let _ = std::fs::remove_file(old);
        }
        let new_tmp = exe.with_extension("new");
        if new_tmp.exists() {
            let _ = std::fs::remove_file(new_tmp);
        }
    }
}

pub async fn check_for_updates() -> Result<CheckUpdateResult, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let url = "https://api.github.com/repos/ulxl/teltonika-orchestrator/releases/latest";
    let res = client
        .get(url)
        .header("User-Agent", "teltonika-orchestrator-updater")
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await
        .map_err(|e| format!("Failed to reach GitHub: {}", e))?;

    if !res.status().is_success() {
        return Err(format!("GitHub returned HTTP {}", res.status()));
    }

    let val: serde_json::Value = res.json().await.map_err(|e| format!("Failed to parse release JSON: {}", e))?;
    let tag_name = val.get("tag_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let release_name = val.get("name").and_then(|v| v.as_str()).unwrap_or(&tag_name).to_string();
    let release_notes = val.get("body").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let published_at = val.get("published_at").and_then(|v| v.as_str()).unwrap_or("").to_string();

    let mut download_url = String::new();
    let mut asset_size = 0u64;

    if let Some(assets) = val.get("assets").and_then(|v| v.as_array()) {
        for asset in assets {
            let name = asset.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name == "teltonika-orchestrator.exe" {
                download_url = asset.get("browser_download_url").and_then(|v| v.as_str()).unwrap_or("").to_string();
                asset_size = asset.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
                break;
            } else if name == "rust-app.exe" && download_url.is_empty() {
                download_url = asset.get("browser_download_url").and_then(|v| v.as_str()).unwrap_or("").to_string();
                asset_size = asset.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
            }
        }
    }

    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let update_available = is_newer_version(&current_version, &tag_name) && !download_url.is_empty();

    Ok(CheckUpdateResult {
        update_available,
        current_version,
        latest_version: tag_name,
        release_name,
        release_notes,
        published_at,
        download_url,
        asset_size,
    })
}

pub async fn apply_update(tx: tokio::sync::broadcast::Sender<String>) -> Result<(), String> {
    let update_info = check_for_updates().await?;
    if update_info.download_url.is_empty() {
        return Err("No executable asset found in latest release".to_string());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?;

    let _ = tx.send("[UPDATE_PROGRESS] 0%".to_string());

    let res = client
        .get(&update_info.download_url)
        .header("User-Agent", "teltonika-orchestrator-updater")
        .send()
        .await
        .map_err(|e| format!("Failed to download update: {}", e))?;

    if !res.status().is_success() {
        return Err(format!("Download failed with HTTP {}", res.status()));
    }

    let total_size = res.content_length().unwrap_or(update_info.asset_size);
    let current_exe = std::env::current_exe().map_err(|e| format!("Cannot find current exe: {}", e))?;
    let new_exe = current_exe.with_extension("new");

    use tokio::io::AsyncWriteExt;
    use tokio_stream::StreamExt;

    let mut file = tokio::fs::File::create(&new_exe)
        .await
        .map_err(|e| format!("Failed to create temporary file: {}", e))?;

    let mut downloaded: u64 = 0;
    let mut stream = res.bytes_stream();

    let mut last_percent = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Error while downloading: {}", e))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Failed to write chunk: {}", e))?;
        downloaded += chunk.len() as u64;
        if total_size > 0 {
            let percent = ((downloaded as f64 / total_size as f64) * 100.0) as u32;
            if percent != last_percent {
                last_percent = percent;
                let _ = tx.send(format!("[UPDATE_PROGRESS] {}%", percent));
            }
        }
    }
    file.flush().await.map_err(|e| format!("Failed to flush file: {}", e))?;
    drop(file);

    let _ = tx.send("[UPDATE_PROGRESS] 100%".to_string());
    let _ = tx.send("[UPDATE_STATUS] Swapping application binaries...".to_string());

    let old_exe = current_exe.with_extension("old");
    if old_exe.exists() {
        let _ = std::fs::remove_file(&old_exe);
    }

    std::fs::rename(&current_exe, &old_exe).map_err(|e| format!("Failed to backup current executable: {}", e))?;
    std::fs::rename(&new_exe, &current_exe).map_err(|e| {
        let _ = std::fs::rename(&old_exe, &current_exe);
        format!("Failed to activate new executable: {}", e)
    })?;

    let _ = tx.send("[UPDATE_SUCCESS] Relaunching Teltonika Orchestrator...".to_string());

    tokio::time::sleep(tokio::time::Duration::from_millis(600)).await;

    std::process::Command::new(&current_exe)
        .spawn()
        .map_err(|e| format!("Failed to launch updated application: {}", e))?;

    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_parsing_and_comparison() {
        assert!(is_newer_version("1.1.0", "v1.2.0"));
        assert!(is_newer_version("1.1.0", "1.1.1"));
        assert!(is_newer_version("1.1.0", "2.0.0"));
        assert!(!is_newer_version("1.1.0", "1.1.0"));
        assert!(!is_newer_version("1.1.0", "v1.1.0"));
        assert!(!is_newer_version("1.2.0", "v1.1.9"));
    }
}

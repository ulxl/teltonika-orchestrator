use axum::{
    extract::{DefaultBodyLimit, Multipart, Path, State},
    response::{
        sse::{Event, KeepAlive, Sse},
        Response,
    },
    routing::{get, post},
    Json, Router,
};
use futures::stream::Stream;
use serde_json::json;
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::config::{load_config, save_config, AppConfig};
use crate::teltonika::{run_automation_sequence, DeviceRecord, JobParams};

#[derive(Clone)]
pub struct AppState {
    pub is_running: Arc<AtomicBool>,
    pub tx_stream: broadcast::Sender<String>,
    pub results_store: Arc<Mutex<HashMap<String, DeviceRecord>>>,
    pub all_input_ips: Arc<Mutex<Vec<String>>>,
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(serve_index))
        .route("/app.js", get(serve_js))
        .route("/api/templates", get(get_templates).post(update_templates))
        .route("/api/setup", post(complete_setup))
        .route("/api/browse-folder", post(browse_folder))
        .route("/api/open-workspace-dir", post(open_workspace_dir))
        .route("/api/open-templates-dir", post(open_templates_dir))
        .route("/api/open-url", post(open_external_url))
        .route("/api/check-update", get(check_update_endpoint))
        .route("/api/apply-update", post(apply_update_endpoint))
        .route("/api/start", post(start_job))
        .route("/api/stop", post(stop_job))
        .route("/api/reboot", post(reboot_single_endpoint))
        .route("/api/stream", get(stream_logs))
        .route("/api/open/{filename}", get(open_file))
        .route("/api/open-results-dir", post(open_results_dir))
        .layer(DefaultBodyLimit::max(256 * 1024 * 1024))
        .with_state(state)
}

async fn serve_index() -> Response {
    Response::builder()
        .header("content-type", "text/html; charset=utf-8")
        .header("cache-control", "no-cache, no-store, must-revalidate")
        .header("pragma", "no-cache")
        .header("expires", "0")
        .body(include_str!("../ui/index.html").into())
        .unwrap()
}

async fn serve_js() -> Response {
    Response::builder()
        .header("content-type", "application/javascript; charset=utf-8")
        .header("cache-control", "no-cache, no-store, must-revalidate")
        .header("pragma", "no-cache")
        .header("expires", "0")
        .body(include_str!("../ui/app.js").into())
        .unwrap()
}

async fn get_templates() -> Json<AppConfig> {
    Json(load_config())
}

async fn update_templates(Json(payload): Json<AppConfig>) -> Result<Json<serde_json::Value>, Response> {
    match save_config(&payload) {
        Ok(_) => Ok(Json(json!({ "status": "ok" }))),
        Err(e) => Err(Response::builder()
            .status(500)
            .body(format!("Failed to save config: {}", e).into())
            .unwrap()),
    }
}

async fn start_job(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, Response> {
    if state.is_running.load(Ordering::Relaxed) {
        return Err(Response::builder()
            .status(400)
            .body("An operation is already in progress".into())
            .unwrap());
    }

    let mut mode = "verify".to_string();
    let mut device_type = "router".to_string();
    let mut ips_raw = String::new();
    let mut passwords_raw = String::new();
    let mut mac_raw = String::new();
    let mut username = "admin".to_string();
    let mut port = "443".to_string();
    let mut workers = 5usize;
    let mut is_retry = false;
    let mut firmware_path_opt: Option<String> = None;
    let mut backup_path_opt: Option<String> = None;
    let mut standard_password_override: Option<String> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "mode" {
            mode = field.text().await.unwrap_or_default();
        } else if name == "deviceType" {
            device_type = field.text().await.unwrap_or_default();
        } else if name == "ips" {
            ips_raw = field.text().await.unwrap_or_default();
        } else if name == "passwords" {
            passwords_raw = field.text().await.unwrap_or_default();
        } else if name == "mac_addresses" {
            mac_raw = field.text().await.unwrap_or_default();
        } else if name == "username" {
            username = field.text().await.unwrap_or_default();
        } else if name == "port" {
            port = field.text().await.unwrap_or_default();
        } else if name == "workers" {
            if let Ok(w) = field.text().await.unwrap_or_default().parse::<usize>() {
                workers = w.clamp(1, 50);
            }
        } else if name == "is_retry" {
            is_retry = field.text().await.unwrap_or_default() == "true";
        } else if name == "firmware_file" {
            let bytes = field.bytes().await.unwrap_or_default();
            if !bytes.is_empty() {
                let _ = std::fs::create_dir_all("results");
                let save_path = "results/firmware_upload.bin".to_string();
                if std::fs::write(&save_path, &bytes).is_ok() {
                    firmware_path_opt = Some(save_path);
                }
            }
        } else if name == "backup_file" {
            let bytes = field.bytes().await.unwrap_or_default();
            if !bytes.is_empty() {
                let _ = std::fs::create_dir_all("results");
                let save_path = "results/backup_upload.tar.gz".to_string();
                if std::fs::write(&save_path, &bytes).is_ok() {
                    backup_path_opt = Some(save_path);
                }
            }
        } else if name == "standard_password" {
            let p = field.text().await.unwrap_or_default().trim().to_string();
            if !p.is_empty() {
                standard_password_override = Some(p);
            }
        }
    }

    if mode == "update_firmware" {
        if username.trim().is_empty() || username == "admin" {
            username = "root".to_string();
        }
        if port.trim().is_empty() || port == "443" || port == "8443" {
            port = "22".to_string();
        }
        if firmware_path_opt.is_none() && std::path::Path::new("results/firmware_upload.bin").exists() {
            firmware_path_opt = Some("results/firmware_upload.bin".to_string());
        }
    } else if mode == "restore_config" {
        if backup_path_opt.is_none() && std::path::Path::new("results/backup_upload.tar.gz").exists() {
            backup_path_opt = Some("results/backup_upload.tar.gz".to_string());
        }
    }

    let ips: Vec<String> = ips_raw
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    if ips.is_empty() {
        return Err(Response::builder()
            .status(400)
            .body("No IP addresses provided".into())
            .unwrap());
    }

    let is_bundle_mode = mode == "bundle_check" || mode == "reconnect_rms_bundle";
    let target_items: Vec<String> = if is_bundle_mode && !is_retry {
        let mut exp = Vec::new();
        for ip in &ips {
            let clean_ip = ip.split(':').next().unwrap_or(ip);
            for p in [6443, 8443, 9443] {
                exp.push(format!("{}:{}", clean_ip, p));
            }
        }
        exp
    } else {
        ips.clone()
    };

    let mut passwords: Vec<String> = passwords_raw
        .split(',')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();

    let cfg = load_config();
    let mut standard_passwords = cfg.standard_passwords.clone();
    if let Some(target_pwd) = standard_password_override {
        standard_passwords.retain(|p| p != &target_pwd);
        standard_passwords.insert(0, target_pwd);
    }
    if passwords.is_empty() {
        passwords = standard_passwords.clone();
    }

    let mac_addresses: Vec<String> = mac_raw
        .lines()
        .map(|m| m.trim().to_uppercase())
        .filter(|m| !m.is_empty())
        .collect();

    if !is_retry {
        let mut all_ips_lock = state.all_input_ips.lock().await;
        *all_ips_lock = target_items.clone();
        state.results_store.lock().await.clear();
    }

    let all_saved_ips = state.all_input_ips.lock().await.clone();

    let job_params = Arc::new(JobParams {
        mode,
        device_type,
        ips: target_items,
        all_input_ips: all_saved_ips,
        passwords,
        standard_passwords,
        mac_addresses,
        username,
        port,
        workers,
        firmware_path: firmware_path_opt,
        backup_file_path: backup_path_opt,
    });

    state.is_running.store(true, Ordering::Relaxed);

    let is_running_cloned = state.is_running.clone();
    let results_store_cloned = state.results_store.clone();
    let tx_cloned = state.tx_stream.clone();

    tokio::spawn(async move {
        if let Err(e) = run_automation_sequence(job_params, is_running_cloned.clone(), results_store_cloned, tx_cloned.clone()).await {
            let _ = tx_cloned.send(format!("[ERROR] Sequence failed: {}", e));
            let _ = tx_cloned.send("DONE:".to_string());
        }
        is_running_cloned.store(false, Ordering::Relaxed);
    });

    Ok(Json(json!({ "message": "Sequence started" })))
}

async fn stop_job(State(state): State<AppState>) -> Json<serde_json::Value> {
    state.is_running.store(false, Ordering::Relaxed);
    let _ = state.tx_stream.send("CANCEL_REQUESTED".to_string());
    let _ = state.tx_stream.send("LIVE:Sequence cancelled by user.".to_string());
    Json(json!({ "message": "Stopped" }))
}

async fn stream_logs(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.tx_stream.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|msg| match msg {
        Ok(line) => Some(Ok(Event::default().data(line))),
        Err(_) => None,
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn open_file(Path(filename): Path<String>) -> Result<Json<serde_json::Value>, Response> {
    if !filename.ends_with(".xlsx") || filename.contains('/') || filename.contains('\\') {
        return Err(Response::builder().status(400).body("Invalid filename".into()).unwrap());
    }
    let cfg = load_config();
    let dir = if cfg.results_dir.trim().is_empty() { "results".to_string() } else { cfg.results_dir };
    let filepath = std::path::Path::new(&dir).join(&filename);
    if !filepath.exists() {
        return Err(Response::builder().status(404).body("File not found".into()).unwrap());
    }
    match open::that(&filepath) {
        Ok(_) => Ok(Json(json!({ "message": "File opened" }))),
        Err(e) => Err(Response::builder().status(500).body(format!("Failed to open file: {}", e).into()).unwrap()),
    }
}

async fn open_results_dir() -> Result<Json<serde_json::Value>, Response> {
    let cfg = load_config();
    let dir = if !cfg.results_dir.trim().is_empty() {
        cfg.results_dir
    } else if !cfg.workspace_dir.trim().is_empty() {
        std::path::Path::new(&cfg.workspace_dir).join("results").to_string_lossy().to_string()
    } else {
        "results".to_string()
    };
    let path = std::path::Path::new(&dir);
    if !path.exists() {
        let _ = std::fs::create_dir_all(path);
    }
    match open::that(path) {
        Ok(_) => Ok(Json(json!({ "message": "Folder opened" }))),
        Err(e) => Err(Response::builder().status(500).body(format!("Failed to open folder: {}", e).into()).unwrap()),
    }
}

async fn open_workspace_dir() -> Result<Json<serde_json::Value>, Response> {
    let cfg = load_config();
    let dir = if !cfg.workspace_dir.trim().is_empty() {
        cfg.workspace_dir
    } else if let Some(ws) = crate::config::get_workspace_dir() {
        ws.to_string_lossy().to_string()
    } else {
        ".".to_string()
    };
    let path = std::path::Path::new(&dir);
    if !path.exists() {
        let _ = std::fs::create_dir_all(path);
    }
    match open::that(path) {
        Ok(_) => Ok(Json(json!({ "message": "Folder opened" }))),
        Err(e) => Err(Response::builder().status(500).body(format!("Failed to open folder: {}", e).into()).unwrap()),
    }
}

#[derive(Debug, serde::Deserialize)]
struct SetupPayload {
    workspace_dir: String,
}

async fn complete_setup(Json(payload): Json<SetupPayload>) -> Result<Json<serde_json::Value>, Response> {
    let ws_dir = payload.workspace_dir.trim().to_string();
    if ws_dir.is_empty() {
        return Err(Response::builder().status(400).body("Workspace directory cannot be empty".into()).unwrap());
    }

    let mut cfg = load_config();
    cfg.is_first_run = false;
    cfg.workspace_dir = ws_dir.clone();
    cfg.results_dir = std::path::Path::new(&ws_dir).join("results").to_string_lossy().to_string();

    match save_config(&cfg) {
        Ok(_) => Ok(Json(json!({ "status": "ok" }))),
        Err(e) => Err(Response::builder().status(500).body(format!("Failed to save setup: {}", e).into()).unwrap()),
    }
}

async fn browse_folder() -> Json<serde_json::Value> {
    let selected = tokio::task::spawn_blocking(|| {
        rfd::FileDialog::new()
            .set_title("Select Workspace Folder")
            .pick_folder()
            .map(|p| p.to_string_lossy().to_string())
    })
    .await
    .unwrap_or(None);

    Json(json!({ "path": selected }))
}

async fn open_templates_dir() -> Result<Json<serde_json::Value>, Response> {
    let cfg = load_config();
    let dir = if !cfg.workspace_dir.trim().is_empty() {
        std::path::Path::new(&cfg.workspace_dir).join("config").to_string_lossy().to_string()
    } else {
        "config".to_string()
    };
    let path = std::path::Path::new(&dir);
    if !path.exists() {
        let _ = std::fs::create_dir_all(path);
    }
    match open::that(path) {
        Ok(_) => Ok(Json(json!({ "message": "Folder opened" }))),
        Err(e) => Err(Response::builder().status(500).body(format!("Failed to open folder: {}", e).into()).unwrap()),
    }
}

#[derive(Debug, serde::Deserialize)]
struct OpenUrlPayload {
    url: String,
}

async fn open_external_url(Json(payload): Json<OpenUrlPayload>) -> Result<Json<serde_json::Value>, Response> {
    let url = payload.url.trim().to_string();
    if url.starts_with("http://") || url.starts_with("https://") {
        match open::that(&url) {
            Ok(_) => Ok(Json(json!({ "status": "ok" }))),
            Err(e) => Err(Response::builder()
                .status(500)
                .header("content-type", "application/json")
                .body(json!({ "status": "error", "message": format!("Failed to open URL: {}", e) }).to_string().into())
                .unwrap()),
        }
    } else {
        Err(Response::builder()
            .status(400)
            .header("content-type", "application/json")
            .body(json!({ "status": "error", "message": "Invalid URL" }).to_string().into())
            .unwrap())
    }
}

async fn check_update_endpoint() -> Result<Json<crate::updater::CheckUpdateResult>, Response> {
    match crate::updater::check_for_updates().await {
        Ok(res) => Ok(Json(res)),
        Err(e) => Err(Response::builder()
            .status(500)
            .header("content-type", "application/json")
            .body(json!({ "status": "error", "message": e }).to_string().into())
            .unwrap()),
    }
}

async fn apply_update_endpoint(State(state): State<AppState>) -> Result<Json<serde_json::Value>, Response> {
    let tx = state.tx_stream.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::updater::apply_update(tx.clone()).await {
            let _ = tx.send(format!("[UPDATE_ERROR] {}", e));
        }
    });
    Ok(Json(json!({ "status": "started" })))
}

#[derive(Debug, serde::Deserialize)]
struct RebootPayload {
    ip: String,
    username: Option<String>,
    port: Option<String>,
    passwords: Option<Vec<String>>,
}

async fn reboot_single_endpoint(
    State(state): State<AppState>,
    Json(payload): Json<RebootPayload>,
) -> Result<Json<serde_json::Value>, Response> {
    let ip = payload.ip.trim().to_string();
    if ip.is_empty() {
        return Err(Response::builder()
            .status(400)
            .header("content-type", "application/json")
            .body(json!({ "status": "error", "message": "IP address cannot be empty" }).to_string().into())
            .unwrap());
    }

    let username = payload.username.unwrap_or_else(|| "admin".to_string());
    let port = payload.port.unwrap_or_else(|| "443".to_string());

    let cfg = load_config();
    let mut candidate_passwords = Vec::new();

    if let Some(pwds) = payload.passwords {
        for p in pwds {
            let trimmed = p.trim().to_string();
            if !trimmed.is_empty() && !candidate_passwords.contains(&trimmed) {
                candidate_passwords.push(trimmed);
            }
        }
    }

    if let Some(tmpl) = cfg.templates.iter().find(|t| t.name == cfg.active_template) {
        for p in &tmpl.passwords {
            if !candidate_passwords.contains(p) {
                candidate_passwords.push(p.clone());
            }
        }
    }

    for p in &cfg.standard_passwords {
        if !candidate_passwords.contains(p) {
            candidate_passwords.push(p.clone());
        }
    }

    let client = crate::teltonika::create_http_client();
    let tx = state.tx_stream.clone();

    match crate::teltonika::reboot_device_direct(&client, ip, &username, &candidate_passwords, &port, tx).await {
        Ok(msg) => Ok(Json(json!({ "status": "ok", "message": msg }))),
        Err(err) => Err(Response::builder()
            .status(500)
            .header("content-type", "application/json")
            .body(json!({ "status": "error", "message": format!("Reboot failed: {}", err) }).to_string().into())
            .unwrap()),
    }
}

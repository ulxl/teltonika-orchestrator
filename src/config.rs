use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordTemplate {
    pub name: String,
    pub passwords: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub active_template: String,
    pub templates: Vec<PasswordTemplate>,
    pub standard_passwords: Vec<String>,
    #[serde(default = "default_results_dir")]
    pub results_dir: String,
    #[serde(default)]
    pub workspace_dir: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_font_size")]
    pub font_size: String,
    #[serde(default = "default_font_family")]
    pub font_family: String,
    #[serde(default = "default_workers")]
    pub default_workers: usize,
    #[serde(default)]
    pub is_first_run: bool,
    #[serde(default)]
    pub suggested_workspace_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspacePointer {
    pub workspace_dir: String,
}

fn default_results_dir() -> String {
    "results".to_string()
}

fn default_theme() -> String {
    "system".to_string()
}

fn default_font_size() -> String {
    "normal".to_string()
}

fn default_font_family() -> String {
    "geist".to_string()
}

fn default_workers() -> usize {
    5
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            active_template: "Default".to_string(),
            templates: vec![
                PasswordTemplate {
                    name: "Default".to_string(),
                    passwords: vec![
                        "Admin123!".to_string(),
                        "Admin123".to_string(),
                    ],
                },
            ],
            standard_passwords: vec!["Admin123!".to_string(), "Admin123".to_string()],
            results_dir: "results".to_string(),
            workspace_dir: String::new(),
            theme: "system".to_string(),
            font_size: "normal".to_string(),
            font_family: "geist".to_string(),
            default_workers: 5,
            is_first_run: false,
            suggested_workspace_dir: String::new(),
        }
    }
}

pub fn get_appdata_dir() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        PathBuf::from(appdata).join("TeltonikaOrchestrator")
    } else if let Ok(userprofile) = std::env::var("USERPROFILE") {
        PathBuf::from(userprofile).join("AppData").join("Roaming").join("TeltonikaOrchestrator")
    } else {
        PathBuf::from("TeltonikaOrchestrator")
    }
}

pub fn get_workspace_dir() -> Option<PathBuf> {
    let pointer_file = get_appdata_dir().join("workspace.json");
    if pointer_file.exists() {
        if let Ok(content) = fs::read_to_string(&pointer_file) {
            if let Ok(ptr) = serde_json::from_str::<WorkspacePointer>(&content) {
                let p = PathBuf::from(&ptr.workspace_dir);
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }
    None
}

pub fn load_config() -> AppConfig {
    if let Some(ws) = get_workspace_dir() {
        let t_path = ws.join("config").join("templates.json");
        if let Ok(content) = fs::read_to_string(&t_path) {
            if let Ok(mut cfg) = serde_json::from_str::<AppConfig>(&content) {
                cfg.is_first_run = false;
                cfg.workspace_dir = ws.to_string_lossy().to_string();
                cfg.results_dir = ws.join("results").to_string_lossy().to_string();
                return cfg;
            }
        }
    }

    let user_profile = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\Teltonika".to_string());
    let suggested_ws = Path::new(&user_profile).join("Documents").join("Teltonika").to_string_lossy().to_string();

    let mut default_cfg = AppConfig::default();
    if let Ok(local_content) = fs::read_to_string("config/templates.json") {
        if let Ok(local_cfg) = serde_json::from_str::<AppConfig>(&local_content) {
            default_cfg.templates = local_cfg.templates;
            default_cfg.active_template = local_cfg.active_template;
        }
    }

    default_cfg.is_first_run = true;
    default_cfg.workspace_dir = suggested_ws.clone();
    default_cfg.results_dir = Path::new(&suggested_ws).join("results").to_string_lossy().to_string();
    default_cfg.suggested_workspace_dir = suggested_ws;
    default_cfg
}

pub fn save_config(cfg: &AppConfig) -> Result<(), std::io::Error> {
    let ws = if !cfg.workspace_dir.trim().is_empty() {
        PathBuf::from(&cfg.workspace_dir)
    } else if let Some(dir) = get_workspace_dir() {
        dir
    } else {
        let user_profile = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\Teltonika".to_string());
        Path::new(&user_profile).join("Documents").join("Teltonika")
    };

    let config_dir = ws.join("config");
    let results_dir = ws.join("results");
    fs::create_dir_all(&config_dir)?;
    fs::create_dir_all(&results_dir)?;

    let target_file = config_dir.join("templates.json");
    let mut to_save = cfg.clone();
    to_save.workspace_dir = ws.to_string_lossy().to_string();
    to_save.results_dir = results_dir.to_string_lossy().to_string();

    let json = serde_json::to_string_pretty(&to_save).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    fs::write(&target_file, json)?;

    let appdata = get_appdata_dir();
    let _ = fs::create_dir_all(&appdata);
    let ptr = WorkspacePointer {
        workspace_dir: ws.to_string_lossy().to_string(),
    };
    if let Ok(ptr_json) = serde_json::to_string_pretty(&ptr) {
        let _ = fs::write(appdata.join("workspace.json"), ptr_json);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_lifecycle() {
        let appdata_file = get_appdata_dir().join("workspace.json");
        let _ = std::fs::remove_file(&appdata_file);

        let initial_cfg = load_config();
        assert!(initial_cfg.templates.len() >= 1);
        assert!(!initial_cfg.suggested_workspace_dir.is_empty());

        let temp_dir = std::env::temp_dir().join("teltonika_test_ws_lifecycle");
        let mut cfg = AppConfig::default();
        cfg.workspace_dir = temp_dir.to_string_lossy().to_string();
        cfg.results_dir = temp_dir.join("results").to_string_lossy().to_string();
        let res = save_config(&cfg);
        assert!(res.is_ok());
        assert!(temp_dir.join("config").join("templates.json").exists());
        assert!(temp_dir.join("results").exists());

        let loaded = load_config();
        assert!(!loaded.is_first_run);
        assert_eq!(loaded.workspace_dir, temp_dir.to_string_lossy().to_string());

        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::remove_file(&appdata_file);
    }
}

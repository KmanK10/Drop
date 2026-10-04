use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::error::DropError;

pub const DEFAULT_SERVER: &str = "https://drop.kiefermenard.com";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub server_url: String,
    pub username: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            server_url: DEFAULT_SERVER.into(),
            username: String::new(),
        }
    }
}

pub fn default_config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("Drop"))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|p| PathBuf::from(p).join("Library/Application Support/Drop"))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
            if !xdg.is_empty() {
                return Some(PathBuf::from(xdg).join("drop"));
            }
        }
        std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config/drop"))
    }
}

/// Reads the server address and username. Any other field, including a password
/// or content key someone put in the file by hand, is ignored and removed.
pub fn load_settings(path: &Path) -> Settings {
    let Ok(text) = fs::read_to_string(path) else {
        return Settings::default();
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return Settings::default();
    };
    let server_url = value
        .get("serverUrl")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.len() < 500)
        .unwrap_or(DEFAULT_SERVER)
        .to_string();
    let username = value
        .get("username")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| s.len() <= 64 && !s.contains(['\n', '\r']))
        .unwrap_or("")
        .to_string();
    let settings = Settings { server_url, username };
    if !allowed_keys_only(&value) {
        let _ = save_settings(path, &settings);
    }
    settings
}

pub fn save_settings(path: &Path, settings: &Settings) -> Result<(), DropError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| DropError::Message("Couldn't save the server address.".into()))?;
    }
    let body = serde_json::to_vec_pretty(&serde_json::json!({
        "serverUrl": settings.server_url,
        "username": settings.username,
    }))
    .map_err(|_| DropError::Message("Couldn't save the server address.".into()))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &body).map_err(|_| DropError::Message("Couldn't save the server address.".into()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
    }
    fs::rename(&tmp, path).map_err(|_| DropError::Message("Couldn't save the server address.".into()))?;
    Ok(())
}

fn allowed_keys_only(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    object.keys().all(|key| key == "serverUrl" || key == "username")
}

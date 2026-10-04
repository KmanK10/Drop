//! Whether closing the window hides Drop or quits it.
//!
//! On Mac the menu calls this "Close to menu bar". On Windows it is
//! "Close to notification area". The file is `window.json` next to the server
//! address, not the keychain. Missing or unreadable means on. Only an explicit
//! false turns it off. The key stays `closeToMenuBar` so a Mac preference
//! already on disk still loads.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

static CLOSE_TO_MENU_BAR: AtomicBool = AtomicBool::new(true);

const FILE_NAME: &str = "window.json";

pub fn close_to_menu_bar() -> bool {
    CLOSE_TO_MENU_BAR.load(Ordering::Relaxed)
}

pub fn load() {
    let on = pref_path().and_then(|path| fs::read_to_string(path).ok()).map(|text| parse(&text)).unwrap_or(true);
    CLOSE_TO_MENU_BAR.store(on, Ordering::Relaxed);
}

pub fn toggle() -> bool {
    let on = !close_to_menu_bar();
    CLOSE_TO_MENU_BAR.store(on, Ordering::Relaxed);
    if let Some(path) = pref_path() {
        let _ = save(&path, on);
    }
    on
}

fn pref_path() -> Option<PathBuf> {
    drop_core::default_config_dir().map(|dir| dir.join(FILE_NAME))
}

fn save(path: &std::path::Path, on: bool) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = if on {
        "{\n  \"closeToMenuBar\": true\n}\n"
    } else {
        "{\n  \"closeToMenuBar\": false\n}\n"
    };
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, body)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
    }
    fs::rename(tmp, path)
}

fn parse(text: &str) -> bool {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    !compact.contains("\"closeToMenuBar\":false")
}

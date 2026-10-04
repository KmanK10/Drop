use std::path::PathBuf;
use std::sync::mpsc::Receiver;

pub enum TrayAction {
    Open,
    SignOut,
    Quit,
    /// Mac menu-bar item. `true` turns Touch ID on, `false` deletes the keychain item.
    #[cfg(target_os = "macos")]
    SetBiometric(bool),
    Dropped(Vec<PathBuf>),
}

pub struct TrayPorts {
    pub events: Receiver<TrayAction>,
    pub set_tooltip: Box<dyn Fn(String) + Send>,
    pub shutdown: Box<dyn FnOnce() + Send>,
}

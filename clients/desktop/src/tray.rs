use std::path::PathBuf;
use std::sync::mpsc::Receiver;

pub enum TrayAction {
    Open,
    SignOut,
    Quit,
    /// `true` turns biometrics on, `false` deletes that keychain item.
    SetBiometric(bool),
    /// `true` asks for a new PIN, `false` deletes the stored PIN wrap.
    SetPin(bool),
    Dropped(Vec<PathBuf>),
}

pub struct TrayPorts {
    pub events: Receiver<TrayAction>,
    pub set_tooltip: Box<dyn Fn(String) + Send>,
    pub shutdown: Box<dyn FnOnce() + Send>,
}

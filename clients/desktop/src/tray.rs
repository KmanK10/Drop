use std::path::PathBuf;
use std::sync::mpsc::Receiver;

pub enum TrayAction {
    Open,
    SignOut,
    Quit,
    Dropped(Vec<PathBuf>),
}

pub struct TrayPorts {
    pub events: Receiver<TrayAction>,
    pub set_tooltip: Box<dyn Fn(String) + Send>,
    pub shutdown: Box<dyn FnOnce() + Send>,
}

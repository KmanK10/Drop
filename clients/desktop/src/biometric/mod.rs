//! Optional biometric unlock. Off until the signed-in user turns it on.
//! The content-key blob is stored in the platform keychain, never in a file.

pub enum BiometricError {
    Canceled,
    Failed(String),
}

#[cfg(target_os = "macos")]
mod touch_id;
#[cfg(target_os = "windows")]
mod hello;

#[cfg(target_os = "macos")]
pub use touch_id::{available, delete, enrolled, label, load, store};
#[cfg(target_os = "windows")]
pub use hello::{available, delete, enrolled, label, load, store};

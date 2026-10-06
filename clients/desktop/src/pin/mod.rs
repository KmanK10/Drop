//! Optional PIN unlock. Off until a signed-in user sets a PIN.
//!
//! The stored value is an Argon2id/AES-GCM wrap of the unlock blob. The PIN
//! and the content key are not written in the clear. The wrap lives in the
//! platform credential store: the Mac login keychain, or Windows Credential
//! Manager. A wrong PIN fails the cipher and does not delete the item.

use drop_core::{pin_rejection, unwrap_pin, wrap_pin};
use zeroize::Zeroizing;

#[cfg(target_os = "macos")]
mod keychain;
#[cfg(target_os = "windows")]
mod credential;

#[cfg(target_os = "macos")]
use keychain as platform;
#[cfg(target_os = "windows")]
use credential as platform;

pub enum PinError {
    Wrong,
    Failed(String),
}

pub fn enrolled() -> bool {
    platform::exists()
}

pub fn store(pin: &str, secret: &[u8]) -> Result<(), PinError> {
    if let Some(problem) = pin_rejection(pin, None) {
        return Err(PinError::Failed(problem.into()));
    }
    let wrapped = wrap_pin(pin, secret).map_err(|error| PinError::Failed(error.to_string()))?;
    platform::write(&wrapped)
}

pub fn open(pin: &str) -> Result<Zeroizing<Vec<u8>>, PinError> {
    let blob = platform::read()?;
    match unwrap_pin(pin, &blob) {
        Ok(plain) => Ok(plain),
        Err(_) => Err(PinError::Wrong),
    }
}

pub fn delete() {
    platform::remove();
}

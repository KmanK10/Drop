//! PIN wrap for an unlock blob.
//!
//! The PIN is stretched with Argon2id into an AES-256-GCM key. The stored
//! bytes are salt plus ciphertext. The PIN and the content key are not written
//! here; the app puts the ciphertext in the platform credential store.

use zeroize::{Zeroize, Zeroizing};

use crate::crypto::{decrypt, encrypt, normalize_password, KDF_MEMORY, KDF_PARALLELISM, KDF_TIME};
use crate::error::DropError;

const MAGIC: &[u8; 4] = b"DRPP";
const VERSION: u8 = 1;

pub fn pin_rejection(pin: &str, confirm: Option<&str>) -> Option<&'static str> {
    let normalized = normalize_password(pin);
    let count = normalized.chars().count();
    if !(4..=8).contains(&count) || !normalized.chars().all(|ch| ch.is_ascii_digit()) {
        return Some("Use 4 to 8 digits.");
    }
    if let Some(confirm) = confirm {
        if normalize_password(confirm) != normalized {
            return Some("Those PINs don't match.");
        }
    }
    None
}

/// Salt and ciphertext. The PIN is not in the result.
pub fn wrap_pin(pin: &str, secret: &[u8]) -> Result<Vec<u8>, DropError> {
    if pin_rejection(pin, None).is_some() {
        return Err(DropError::Message("Use 4 to 8 digits.".into()));
    }
    let mut salt = vec![0u8; 16];
    getrandom::getrandom(&mut salt).map_err(|_| DropError::Message("Couldn't store the PIN.".into()))?;
    let mut key = pin_key(pin, &salt)?;
    let sealed = match encrypt(&key, secret) {
        Ok(sealed) => sealed,
        Err(error) => {
            key.zeroize();
            return Err(error);
        }
    };
    key.zeroize();
    let mut out = Vec::with_capacity(5 + salt.len() + sealed.len());
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    out.extend_from_slice(&salt);
    out.extend_from_slice(&sealed);
    Ok(out)
}

/// A wrong PIN fails the cipher and leaves the stored blob alone.
pub fn unwrap_pin(pin: &str, blob: &[u8]) -> Result<Zeroizing<Vec<u8>>, DropError> {
    if blob.len() < 4 + 1 + 16 + 12 + 16 || &blob[..4] != MAGIC || blob[4] != VERSION {
        return Err(DropError::Message("That PIN is wrong.".into()));
    }
    let salt = &blob[5..21];
    let sealed = &blob[21..];
    let mut key = pin_key(pin, salt)?;
    let plain = match decrypt(&key, sealed) {
        Ok(plain) => plain,
        Err(_) => {
            key.zeroize();
            return Err(DropError::Message("That PIN is wrong.".into()));
        }
    };
    key.zeroize();
    Ok(Zeroizing::new(plain))
}

fn pin_key(pin: &str, salt: &[u8]) -> Result<Zeroizing<[u8; 32]>, DropError> {
    if salt.len() != 16 {
        return Err(DropError::Message("That PIN is wrong.".into()));
    }
    let normalized = Zeroizing::new(normalize_password(pin));
    let params = argon2::Params::new(KDF_MEMORY, KDF_TIME, KDF_PARALLELISM, Some(32))
        .map_err(|_| DropError::Message("Couldn't derive the PIN key.".into()))?;
    let argon = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut key = Zeroizing::new([0u8; 32]);
    argon
        .hash_password_into(normalized.as_bytes(), salt, key.as_mut())
        .map_err(|_| DropError::Message("Couldn't derive the PIN key.".into()))?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_rules_are_four_to_eight_digits() {
        assert_eq!(pin_rejection("123", None), Some("Use 4 to 8 digits."));
        assert_eq!(pin_rejection("123456789", None), Some("Use 4 to 8 digits."));
        assert_eq!(pin_rejection("12ab", None), Some("Use 4 to 8 digits."));
        assert_eq!(pin_rejection("1234", Some("9999")), Some("Those PINs don't match."));
        assert_eq!(pin_rejection("1234", Some("1234")), None);
        assert_eq!(pin_rejection("１２３４", Some("1234")), None);
    }

    #[test]
    fn pin_wrap_hides_the_secret_and_rejects_the_wrong_pin() {
        let secret = b"pin-secret-marker-not-the-password";
        let wrapped = wrap_pin("1234", secret).unwrap();
        assert_eq!(&wrapped[..4], b"DRPP");
        assert!(!wrapped.windows(secret.len()).any(|window| window == secret));
        assert!(!wrapped.windows(4).any(|window| window == b"1234"));
        let opened = unwrap_pin("1234", &wrapped).unwrap();
        assert_eq!(opened.as_slice(), secret);
        assert!(unwrap_pin("9999", &wrapped).is_err());
    }
}

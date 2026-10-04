//! Bytes that a biometric keychain can hold so a later unlock does not need the
//! password. This module only builds that blob in memory. It does not write a
//! file, and it does not store the password.

use zeroize::{Zeroize, Zeroizing};

use crate::error::DropError;

const MAGIC: &[u8; 4] = b"DRPK";
const VERSION: u8 = 1;

/// Content key plus the session cookie that lets that key talk to the server.
/// The password is not part of this.
pub struct UnlockMaterial {
    pub content_key: Zeroizing<[u8; 32]>,
    pub server_url: String,
    pub username: String,
    pub cookie: Zeroizing<String>,
}

impl UnlockMaterial {
    pub fn encode(&self) -> Result<Zeroizing<Vec<u8>>, DropError> {
        if self.server_url.is_empty() || self.server_url.len() > 500 {
            return Err(DropError::Message("Couldn't store biometric unlock.".into()));
        }
        if self.username.is_empty() || self.username.len() > 64 {
            return Err(DropError::Message("Couldn't store biometric unlock.".into()));
        }
        if self.cookie.is_empty() || self.cookie.len() > 4096 {
            return Err(DropError::Message("Couldn't store biometric unlock.".into()));
        }
        let mut out = Vec::with_capacity(4 + 1 + 32 + self.server_url.len() + self.username.len() + self.cookie.len() + 6);
        out.extend_from_slice(MAGIC);
        out.push(VERSION);
        out.extend_from_slice(self.content_key.as_slice());
        push_text(&mut out, &self.server_url)?;
        push_text(&mut out, &self.username)?;
        push_text(&mut out, self.cookie.as_str())?;
        Ok(Zeroizing::new(out))
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, DropError> {
        if bytes.len() < 4 + 1 + 32 + 6 || &bytes[..4] != MAGIC || bytes[4] != VERSION {
            return Err(DropError::Message("Couldn't unlock with biometrics.".into()));
        }
        let mut content_key = Zeroizing::new([0u8; 32]);
        content_key.copy_from_slice(&bytes[5..37]);
        let mut index = 37;
        let server_url = take_text(bytes, &mut index)?;
        let username = take_text(bytes, &mut index)?;
        let cookie = Zeroizing::new(take_text(bytes, &mut index)?);
        if index != bytes.len() || server_url.is_empty() || username.is_empty() || cookie.is_empty() {
            content_key.zeroize();
            return Err(DropError::Message("Couldn't unlock with biometrics.".into()));
        }
        Ok(Self {
            content_key,
            server_url,
            username,
            cookie,
        })
    }
}

fn push_text(out: &mut Vec<u8>, text: &str) -> Result<(), DropError> {
    let len = u16::try_from(text.len()).map_err(|_| DropError::Message("Couldn't store biometric unlock.".into()))?;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(text.as_bytes());
    Ok(())
}

fn take_text(bytes: &[u8], index: &mut usize) -> Result<String, DropError> {
    if *index + 2 > bytes.len() {
        return Err(DropError::Message("Couldn't unlock with biometrics.".into()));
    }
    let len = u16::from_le_bytes([bytes[*index], bytes[*index + 1]]) as usize;
    *index += 2;
    if len > 4096 || *index + len > bytes.len() {
        return Err(DropError::Message("Couldn't unlock with biometrics.".into()));
    }
    let text = std::str::from_utf8(&bytes[*index..*index + len])
        .map_err(|_| DropError::Message("Couldn't unlock with biometrics.".into()))?
        .to_string();
    *index += len;
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> UnlockMaterial {
        UnlockMaterial {
            content_key: Zeroizing::new([7u8; 32]),
            server_url: "https://drop.example".into(),
            username: "ada".into(),
            cookie: Zeroizing::new("session-token".into()),
        }
    }

    #[test]
    fn unlock_blob_round_trips_without_the_password() {
        let encoded = sample().encode().unwrap();
        assert_eq!(&encoded[..4], b"DRPK");
        assert!(!encoded.windows(8).any(|window| window == b"password"));
        let decoded = UnlockMaterial::decode(&encoded).unwrap();
        assert_eq!(decoded.content_key.as_slice(), &[7u8; 32]);
        assert_eq!(decoded.server_url, "https://drop.example");
        assert_eq!(decoded.username, "ada");
        assert_eq!(decoded.cookie.as_str(), "session-token");
    }

    #[test]
    fn unlock_blob_rejects_a_truncated_buffer() {
        let encoded = sample().encode().unwrap();
        assert!(UnlockMaterial::decode(&encoded[..encoded.len() - 1]).is_err());
        assert!(UnlockMaterial::decode(b"DRPK").is_err());
    }
}

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use unicode_normalization::UnicodeNormalization;
use zeroize::{Zeroize, Zeroizing};

use crate::bytes::{b64url_to_bytes, bytes_to_b64url, sha256, timing_equal};
use crate::error::DropError;

pub const KDF_MEMORY: u32 = 19_456;
pub const KDF_TIME: u32 = 2;
pub const KDF_PARALLELISM: u32 = 1;
pub const KEY_CHECK_TEXT: &str = "drop-key-check-v1";

/// Upper bounds so a hostile server cannot ask the client to allocate a huge KDF.
/// The real parameters (19 MiB, 2 iterations, 1 lane) sit inside this range.
const KDF_MEMORY_CAP: u32 = 262_144;
const KDF_TIME_CAP: u32 = 16;
const KDF_PARALLELISM_CAP: u32 = 4;

#[derive(Debug, Clone)]
pub struct KdfParams {
    pub algo: String,
    pub memory: u32,
    pub time: u32,
    pub parallelism: u32,
}

pub struct Derived {
    pub auth_verifier: Zeroizing<Vec<u8>>,
    pub content_key: Zeroizing<[u8; 32]>,
}

pub struct AccountMaterial {
    pub salt: Vec<u8>,
    pub auth_verifier: Zeroizing<Vec<u8>>,
    pub content_key: Zeroizing<[u8; 32]>,
    pub key_check: Vec<u8>,
}

pub fn normalize_password(password: &str) -> String {
    password.nfkc().collect()
}

/// Same rules as the website. The confirmation is compared after NFKC.
pub fn password_rejection(password: &str, confirm: Option<&str>) -> Option<&'static str> {
    let normalized = normalize_password(password);
    let count = normalized.chars().count();
    if count < 10 {
        return Some("Use at least 10 characters.");
    }
    if count > 200 {
        return Some("That password is too long.");
    }
    if let Some(confirm) = confirm {
        if normalize_password(confirm) != normalized {
            return Some("Those passwords don't match.");
        }
    }
    None
}

pub fn assert_strong_kdf(params: &KdfParams) -> Result<(), DropError> {
    if params.algo != "argon2id" {
        return Err(DropError::BadKdf);
    }
    if params.memory < KDF_MEMORY
        || params.time < KDF_TIME
        || params.parallelism < 1
        || params.memory > KDF_MEMORY_CAP
        || params.time > KDF_TIME_CAP
        || params.parallelism > KDF_PARALLELISM_CAP
    {
        return Err(DropError::WeakKdf);
    }
    Ok(())
}

/// One Argon2id run, 64 bytes. The first 32 are hashed into the auth verifier.
/// The last 32 are the AES-256-GCM content key and never leave this process.
pub fn derive_keys(password: &str, salt: &[u8], params: &KdfParams) -> Result<Derived, DropError> {
    assert_strong_kdf(params)?;
    if salt.len() < 16 || salt.len() > 64 {
        return Err(DropError::BadKdf);
    }
    let normalized = Zeroizing::new(normalize_password(password));
    if normalized.chars().count() > 200 {
        return Err(DropError::LongPassword);
    }
    let argon_params = Params::new(params.memory, params.time, params.parallelism, Some(64))
        .map_err(|_| DropError::WeakKdf)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params);
    let mut derived = Zeroizing::new([0u8; 64]);
    argon
        .hash_password_into(normalized.as_bytes(), salt, derived.as_mut())
        .map_err(|_| DropError::Message("Couldn't derive the key.".into()))?;
    let mut auth_secret = Zeroizing::new([0u8; 32]);
    auth_secret.copy_from_slice(&derived[..32]);
    let mut content_key = Zeroizing::new([0u8; 32]);
    content_key.copy_from_slice(&derived[32..]);
    derived.zeroize();
    let verifier = sha256(auth_secret.as_slice());
    auth_secret.zeroize();
    Ok(Derived {
        auth_verifier: Zeroizing::new(verifier.to_vec()),
        content_key,
    })
}

pub fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, DropError> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| DropError::Message("Couldn't encrypt that.".into()))?;
    let mut iv = [0u8; 12];
    getrandom::getrandom(&mut iv).map_err(|_| DropError::Message("Couldn't encrypt that.".into()))?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&iv), plaintext)
        .map_err(|_| DropError::Message("Couldn't encrypt that.".into()))?;
    let mut out = Vec::with_capacity(iv.len() + ciphertext.len());
    out.extend_from_slice(&iv);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

pub fn decrypt(key: &[u8; 32], blob: &[u8]) -> Result<Vec<u8>, DropError> {
    if blob.len() < 12 + 16 {
        return Err(DropError::Message("Ciphertext is too short.".into()));
    }
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| DropError::Message("Couldn't decrypt that.".into()))?;
    let (iv, ciphertext) = blob.split_at(12);
    cipher
        .decrypt(Nonce::from_slice(iv), ciphertext)
        .map_err(|_| DropError::Message("Couldn't decrypt that.".into()))
}

pub fn make_key_check(content_key: &[u8; 32]) -> Result<Vec<u8>, DropError> {
    encrypt(content_key, KEY_CHECK_TEXT.as_bytes())
}

pub fn verify_key_check(content_key: &[u8; 32], blob: &[u8]) -> Result<bool, DropError> {
    let plain = match decrypt(content_key, blob) {
        Ok(plain) => plain,
        Err(_) => return Ok(false),
    };
    Ok(timing_equal(&plain, KEY_CHECK_TEXT.as_bytes()))
}

pub fn account_material(password: &str) -> Result<AccountMaterial, DropError> {
    let mut salt = vec![0u8; 16];
    getrandom::getrandom(&mut salt).map_err(|_| DropError::Message("Couldn't start the account.".into()))?;
    let params = KdfParams {
        algo: "argon2id".into(),
        memory: KDF_MEMORY,
        time: KDF_TIME,
        parallelism: KDF_PARALLELISM,
    };
    let derived = derive_keys(password, &salt, &params)?;
    let key_check = make_key_check(&derived.content_key)?;
    Ok(AccountMaterial {
        salt,
        auth_verifier: derived.auth_verifier,
        content_key: derived.content_key,
        key_check,
    })
}

/// Fields the browser is allowed to send when creating an account. No password, no content key.
pub fn registration_body(material: &AccountMaterial) -> serde_json::Value {
    serde_json::json!({
        "authVerifier": bytes_to_b64url(&material.auth_verifier),
        "kdfSalt": bytes_to_b64url(&material.salt),
        "kdfMemory": KDF_MEMORY,
        "kdfTime": KDF_TIME,
        "kdfParallelism": KDF_PARALLELISM,
        "keyCheck": bytes_to_b64url(&material.key_check),
    })
}

pub fn login_body(username: &str, auth_verifier: &[u8]) -> serde_json::Value {
    serde_json::json!({
        "username": username,
        "authVerifier": bytes_to_b64url(auth_verifier),
    })
}

pub fn parse_kdf(value: &serde_json::Value) -> Result<(KdfParams, Vec<u8>), DropError> {
    let algo = value.get("algo").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let memory = json_u32(value.get("memory")).ok_or(DropError::BadKdf)?;
    let time = json_u32(value.get("time")).ok_or(DropError::BadKdf)?;
    let parallelism = json_u32(value.get("parallelism")).ok_or(DropError::BadKdf)?;
    let salt_b64 = value.get("salt").and_then(|v| v.as_str()).ok_or(DropError::BadKdf)?;
    let salt = b64url_to_bytes(salt_b64).map_err(|_| DropError::BadKdf)?;
    let params = KdfParams {
        algo,
        memory,
        time,
        parallelism,
    };
    assert_strong_kdf(&params)?;
    if salt.len() < 16 {
        return Err(DropError::BadKdf);
    }
    Ok((params, salt))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_rules_match_the_browser() {
        assert_eq!(password_rejection("short", Some("short")), Some("Use at least 10 characters."));
        let too_long = "a".repeat(201);
        assert_eq!(password_rejection(&too_long, Some(&too_long)), Some("That password is too long."));
        assert_eq!(
            password_rejection("long-enough-password", Some("different-password")),
            Some("Those passwords don't match.")
        );
        assert_eq!(password_rejection("long-enough-password", Some("long-enough-password")), None);
    }
}

fn json_u32(value: Option<&serde_json::Value>) -> Option<u32> {
    let number = value?.as_u64()?;
    u32::try_from(number).ok()
}

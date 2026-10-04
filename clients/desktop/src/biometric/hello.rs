//! Windows Hello gate. The content-key blob is encrypted to a Hello key and the
//! ciphertext is kept in Credential Manager, not in a normal file.

use std::ptr;

use zeroize::{Zeroize, Zeroizing};
use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::FILETIME;
use windows::Win32::Security::Credentials::{
    CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
};
use windows::Win32::Security::Cryptography::{
    BCryptGenRandom, NCryptCreatePersistedKey, NCryptDecrypt, NCryptDeleteKey, NCryptEncrypt, NCryptFinalizeKey,
    NCryptFreeObject, NCryptOpenKey, NCryptOpenStorageProvider, NCryptSetProperty, BCRYPT_OAEP_PADDING_INFO,
    BCRYPT_RSA_ALGORITHM, BCRYPT_USE_SYSTEM_PREFERRED_RNG, CERT_KEY_SPEC, NCRYPT_FLAGS, NCRYPT_HANDLE,
    NCRYPT_KEY_HANDLE, NCRYPT_LENGTH_PROPERTY, NCRYPT_OVERWRITE_KEY_FLAG, NCRYPT_PAD_OAEP_FLAG, NCRYPT_PROV_HANDLE,
    NCRYPT_SILENT_FLAG,
};

use super::BiometricError;

const TARGET: &str = "com.kiefermenard.drop/unlock";
const KEY_NAME: &str = "DropUnlock";
const NTE_BAD_KEYSET: u32 = 0x8009_0016;
const NTE_SILENT_CONTEXT: u32 = 0x8009_0022;
const NTE_USER_CANCELLED: u32 = 0x8009_0036;
const ERROR_CANCELLED: u32 = 0x8007_04C7;

pub fn label() -> &'static str {
    "Unlock with Windows Hello"
}

pub fn available() -> bool {
    open_provider().is_ok()
}

pub fn enrolled() -> bool {
    let Ok(provider) = open_provider() else {
        return false;
    };
    let mut key = NCRYPT_KEY_HANDLE::default();
    let name = wide(KEY_NAME);
    let status = unsafe {
        NCryptOpenKey(
            provider.0,
            &mut key,
            PCWSTR(name.as_ptr()),
            CERT_KEY_SPEC(0),
            NCRYPT_SILENT_FLAG,
        )
    };
    let code = hresult_code(&status);
    let opened = status.is_ok() || code == NTE_SILENT_CONTEXT;
    if status.is_ok() {
        unsafe {
            let _ = NCryptFreeObject(NCRYPT_HANDLE::from(key));
        }
    }
    opened && credential_exists()
}

pub fn store(secret: &[u8]) -> Result<(), BiometricError> {
    if !available() {
        return Err(BiometricError::Failed("Windows Hello isn't available.".into()));
    }
    let provider = open_provider()?;
    let key = ensure_key(&provider)?;
    let mut aes = Zeroizing::new([0u8; 32]);
    let random = unsafe { BCryptGenRandom(None, aes.as_mut_slice(), BCRYPT_USE_SYSTEM_PREFERRED_RNG) };
    if random.0 < 0 {
        return Err(BiometricError::Failed("Couldn't store Windows Hello unlock.".into()));
    }
    let wrapped = rsa_crypt(&key, aes.as_slice(), true)?;
    let ciphertext = drop_core::encrypt(&*aes, secret)
        .map_err(|_| BiometricError::Failed("Couldn't store Windows Hello unlock.".into()))?;
    aes.zeroize();
    let mut blob = Vec::new();
    blob.extend_from_slice(b"DRPH");
    blob.push(1);
    blob.extend_from_slice(&(wrapped.len() as u32).to_le_bytes());
    blob.extend_from_slice(&wrapped);
    blob.extend_from_slice(&ciphertext);
    write_credential(&blob)?;
    Ok(())
}

pub fn load() -> Result<Zeroizing<Vec<u8>>, BiometricError> {
    let blob = read_credential()?;
    if blob.len() < 9 || &blob[..4] != b"DRPH" || blob[4] != 1 {
        return Err(BiometricError::Failed("Couldn't unlock with Windows Hello.".into()));
    }
    let wrapped_len = u32::from_le_bytes([blob[5], blob[6], blob[7], blob[8]]) as usize;
    if wrapped_len == 0 || 9 + wrapped_len >= blob.len() {
        return Err(BiometricError::Failed("Couldn't unlock with Windows Hello.".into()));
    }
    let wrapped = &blob[9..9 + wrapped_len];
    let ciphertext = &blob[9 + wrapped_len..];
    let provider = open_provider()?;
    let key = open_key(&provider, false)?;
    let mut aes_bytes = rsa_crypt(&key, wrapped, false)?;
    if aes_bytes.len() < 32 {
        aes_bytes.zeroize();
        return Err(BiometricError::Failed("Couldn't unlock with Windows Hello.".into()));
    }
    let mut aes = Zeroizing::new([0u8; 32]);
    aes.copy_from_slice(&aes_bytes[aes_bytes.len() - 32..]);
    aes_bytes.zeroize();
    let plain = drop_core::decrypt(&*aes, ciphertext)
        .map_err(|_| BiometricError::Failed("Couldn't unlock with Windows Hello.".into()))?;
    Ok(Zeroizing::new(plain))
}

pub fn delete() {
    let target = wide(TARGET);
    unsafe {
        let _ = CredDeleteW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, 0);
    }
    if let Ok(provider) = open_provider() {
        let opened = open_key(&provider, true).or_else(|_| open_key(&provider, false));
        if let Ok(mut key) = opened {
            unsafe {
                let _ = NCryptDeleteKey(key.0, 0);
            }
            // Delete frees the handle. Drop must not free it again.
            key.0 = NCRYPT_KEY_HANDLE::default();
        }
    }
}

struct Provider(NCRYPT_PROV_HANDLE);

impl Drop for Provider {
    fn drop(&mut self) {
        if self.0 .0 != 0 {
            unsafe {
                let _ = NCryptFreeObject(NCRYPT_HANDLE(self.0 .0));
            }
        }
    }
}

struct Key(NCRYPT_KEY_HANDLE);

impl Drop for Key {
    fn drop(&mut self) {
        if self.0 .0 != 0 {
            unsafe {
                let _ = NCryptFreeObject(NCRYPT_HANDLE::from(self.0));
            }
        }
    }
}

fn open_provider() -> Result<Provider, BiometricError> {
    let mut provider = NCRYPT_PROV_HANDLE::default();
    let name = wide("Microsoft Passport Key Storage Provider");
    unsafe { NCryptOpenStorageProvider(&mut provider, PCWSTR(name.as_ptr()), 0) }
        .map_err(|_| BiometricError::Failed("Windows Hello isn't available.".into()))?;
    Ok(Provider(provider))
}

fn ensure_key(provider: &Provider) -> Result<Key, BiometricError> {
    match open_key(provider, true) {
        Ok(key) => return Ok(key),
        // The key is already there. Silent open refused it, so ask Hello for that key.
        Err(BiometricError::Canceled) => return open_key(provider, false),
        Err(BiometricError::Failed(_)) => {}
    }
    let mut key = NCRYPT_KEY_HANDLE::default();
    let name = wide(KEY_NAME);
    unsafe {
        NCryptCreatePersistedKey(
            provider.0,
            &mut key,
            BCRYPT_RSA_ALGORITHM,
            PCWSTR(name.as_ptr()),
            CERT_KEY_SPEC(0),
            NCRYPT_OVERWRITE_KEY_FLAG,
        )
        .map_err(map_status)?;
        let bits = 2048u32.to_le_bytes();
        NCryptSetProperty(NCRYPT_HANDLE::from(key), NCRYPT_LENGTH_PROPERTY, &bits, NCRYPT_FLAGS(0))
            .map_err(|_| BiometricError::Failed("Couldn't store Windows Hello unlock.".into()))?;
        NCryptFinalizeKey(key, NCRYPT_FLAGS(0)).map_err(map_status)?;
    }
    Ok(Key(key))
}

fn open_key(provider: &Provider, silent: bool) -> Result<Key, BiometricError> {
    let mut key = NCRYPT_KEY_HANDLE::default();
    let name = wide(KEY_NAME);
    let flags = if silent { NCRYPT_SILENT_FLAG } else { NCRYPT_FLAGS(0) };
    unsafe {
        NCryptOpenKey(provider.0, &mut key, PCWSTR(name.as_ptr()), CERT_KEY_SPEC(0), flags).map_err(map_status)?;
    }
    Ok(Key(key))
}

fn rsa_crypt(key: &Key, input: &[u8], encrypt: bool) -> Result<Vec<u8>, BiometricError> {
    let padding = BCRYPT_OAEP_PADDING_INFO {
        pszAlgId: w!("SHA256"),
        pbLabel: ptr::null_mut(),
        cbLabel: 0,
    };
    let padding_ptr = &padding as *const BCRYPT_OAEP_PADDING_INFO as *const std::ffi::c_void;
    let mut written = 0u32;
    unsafe {
        if encrypt {
            NCryptEncrypt(key.0, Some(input), Some(padding_ptr), None, &mut written, NCRYPT_PAD_OAEP_FLAG)
        } else {
            NCryptDecrypt(key.0, Some(input), Some(padding_ptr), None, &mut written, NCRYPT_PAD_OAEP_FLAG)
        }
        .map_err(map_status)?;
    }
    let mut output = vec![0u8; written as usize];
    unsafe {
        if encrypt {
            NCryptEncrypt(
                key.0,
                Some(input),
                Some(padding_ptr),
                Some(&mut output),
                &mut written,
                NCRYPT_PAD_OAEP_FLAG,
            )
        } else {
            NCryptDecrypt(
                key.0,
                Some(input),
                Some(padding_ptr),
                Some(&mut output),
                &mut written,
                NCRYPT_PAD_OAEP_FLAG,
            )
        }
        .map_err(map_status)?;
    }
    output.truncate(written as usize);
    Ok(output)
}

fn write_credential(blob: &[u8]) -> Result<(), BiometricError> {
    let mut target = wide(TARGET);
    let mut comment = wide("Drop biometric unlock");
    let mut owned = blob.to_vec();
    let credential = CREDENTIALW {
        Flags: Default::default(),
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(target.as_mut_ptr()),
        Comment: PWSTR(comment.as_mut_ptr()),
        LastWritten: FILETIME::default(),
        CredentialBlobSize: owned.len() as u32,
        CredentialBlob: owned.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        AttributeCount: 0,
        Attributes: ptr::null_mut(),
        TargetAlias: PWSTR::null(),
        UserName: PWSTR::null(),
    };
    unsafe { CredWriteW(&credential, 0) }
        .map_err(|_| BiometricError::Failed("Couldn't store Windows Hello unlock.".into()))?;
    owned.zeroize();
    Ok(())
}

fn read_credential() -> Result<Zeroizing<Vec<u8>>, BiometricError> {
    let target = wide(TARGET);
    let mut credential = ptr::null_mut();
    unsafe { CredReadW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, 0, &mut credential) }
        .map_err(|_| BiometricError::Failed("Windows Hello unlock is off.".into()))?;
    if credential.is_null() {
        return Err(BiometricError::Failed("Windows Hello unlock is off.".into()));
    }
    let bytes = unsafe {
        let size = (*credential).CredentialBlobSize as usize;
        let pointer = (*credential).CredentialBlob;
        let mut out = Zeroizing::new(vec![0u8; size]);
        if size > 0 && !pointer.is_null() {
            ptr::copy_nonoverlapping(pointer, out.as_mut_ptr(), size);
        }
        CredFree(credential.cast());
        out
    };
    Ok(bytes)
}

fn credential_exists() -> bool {
    let target = wide(TARGET);
    let mut credential = ptr::null_mut();
    let ok = unsafe { CredReadW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, 0, &mut credential) }.is_ok();
    if !credential.is_null() {
        unsafe { CredFree(credential.cast()) };
    }
    ok
}

fn map_status(error: windows::core::Error) -> BiometricError {
    let code = error.code().0 as u32;
    if code == NTE_USER_CANCELLED || code == ERROR_CANCELLED || code == NTE_SILENT_CONTEXT {
        BiometricError::Canceled
    } else if code == NTE_BAD_KEYSET {
        BiometricError::Failed("Windows Hello unlock is off.".into())
    } else {
        BiometricError::Failed("Couldn't unlock with Windows Hello.".into())
    }
}

fn hresult_code(error: &windows::core::Result<()>) -> u32 {
    match error {
        Ok(()) => 0,
        Err(error) => error.code().0 as u32,
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

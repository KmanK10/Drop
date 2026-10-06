//! Windows Credential Manager holds the PIN wrap. The PIN is not a credential field.

use std::ptr;

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::FILETIME;
use windows::Win32::Security::Credentials::{
    CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
};
use zeroize::{Zeroize, Zeroizing};

use super::PinError;

const TARGET: &str = "com.kiefermenard.drop/pin";

pub fn exists() -> bool {
    let target = wide(TARGET);
    let mut credential = ptr::null_mut();
    let ok = unsafe { CredReadW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, 0, &mut credential) }.is_ok();
    if !credential.is_null() {
        unsafe { CredFree(credential.cast()) };
    }
    ok
}

pub fn write(blob: &[u8]) -> Result<(), PinError> {
    let mut target = wide(TARGET);
    let mut comment = wide("Drop PIN unlock");
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
    unsafe { CredWriteW(&credential, 0) }.map_err(|_| PinError::Failed("Couldn't store the PIN.".into()))?;
    owned.zeroize();
    Ok(())
}

pub fn read() -> Result<Zeroizing<Vec<u8>>, PinError> {
    let target = wide(TARGET);
    let mut credential = ptr::null_mut();
    unsafe { CredReadW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, 0, &mut credential) }
        .map_err(|_| PinError::Failed("PIN unlock is off.".into()))?;
    if credential.is_null() {
        return Err(PinError::Failed("PIN unlock is off.".into()));
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

pub fn remove() {
    let target = wide(TARGET);
    unsafe {
        let _ = CredDeleteW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, 0);
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

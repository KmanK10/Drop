//! Mac login keychain item for the PIN wrap.
//!
//! This is not the Touch ID item. It does not ask for biometrics and it does
//! not use the data-protection keychain, so an unsigned build can store it.
//! The password and the PIN are not in the item. The value is the wrap.

use std::ptr;

use super::PinError;

const SERVICE: &str = "com.kiefermenard.drop.mac.pin";
const ACCOUNT: &str = "unlock";
const ERR_SUCCESS: i32 = 0;
const ERR_DUPLICATE_ITEM: i32 = -25299;

pub fn exists() -> bool {
    let Some(query) = dictionary(false, None) else {
        return false;
    };
    let status = unsafe { SecItemCopyMatching(query.0, ptr::null_mut()) };
    status == ERR_SUCCESS
}

pub fn write(blob: &[u8]) -> Result<(), PinError> {
    let Some(add) = dictionary(false, Some(blob)) else {
        return Err(PinError::Failed("Couldn't store the PIN.".into()));
    };
    let status = unsafe { SecItemAdd(add.0, ptr::null_mut()) };
    if status == ERR_SUCCESS {
        return Ok(());
    }
    if status != ERR_DUPLICATE_ITEM {
        return Err(PinError::Failed(format!("Couldn't store the PIN. Keychain status {status}.")));
    }
    let Some(query) = dictionary(false, None) else {
        return Err(PinError::Failed("Couldn't store the PIN.".into()));
    };
    let Some(update) = value_only(blob) else {
        return Err(PinError::Failed("Couldn't store the PIN.".into()));
    };
    let status = unsafe { SecItemUpdate(query.0, update.0) };
    if status == ERR_SUCCESS {
        Ok(())
    } else {
        Err(PinError::Failed(format!("Couldn't store the PIN. Keychain status {status}.")))
    }
}

pub fn read() -> Result<zeroize::Zeroizing<Vec<u8>>, PinError> {
    let Some(query) = dictionary(true, None) else {
        return Err(PinError::Failed("PIN unlock is off.".into()));
    };
    let mut result: *const u8 = ptr::null();
    let status = unsafe { SecItemCopyMatching(query.0, &mut result) };
    if status != ERR_SUCCESS || result.is_null() {
        return Err(PinError::Failed("PIN unlock is off.".into()));
    }
    let bytes = unsafe {
        let pointer = CFDataGetBytePtr(result);
        let length = CFDataGetLength(result);
        let mut out = zeroize::Zeroizing::new(vec![0u8; length.max(0) as usize]);
        if length > 0 && !pointer.is_null() {
            ptr::copy_nonoverlapping(pointer, out.as_mut_ptr(), out.len());
        }
        CFRelease(result);
        out
    };
    if bytes.is_empty() {
        return Err(PinError::Failed("PIN unlock is off.".into()));
    }
    Ok(bytes)
}

pub fn remove() {
    if let Some(query) = dictionary(false, None) {
        unsafe { SecItemDelete(query.0) };
    }
}

struct Dictionary(*const u8);

impl Drop for Dictionary {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) };
    }
}

fn dictionary(returning_data: bool, secret: Option<&[u8]>) -> Option<Dictionary> {
    unsafe {
        let service = cf_string(SERVICE);
        let account = cf_string(ACCOUNT);
        if service.is_null() || account.is_null() {
            release(service);
            release(account);
            return None;
        }
        let mut keys = vec![
            kSecClass as *const u8,
            kSecAttrService as *const u8,
            kSecAttrAccount as *const u8,
            kSecAttrAccessible as *const u8,
        ];
        let mut values = vec![
            kSecClassGenericPassword as *const u8,
            service,
            account,
            kSecAttrAccessibleWhenUnlockedThisDeviceOnly as *const u8,
        ];
        let mut data = ptr::null();
        if let Some(secret) = secret {
            data = CFDataCreate(ptr::null(), secret.as_ptr(), secret.len() as isize);
            if data.is_null() {
                release(service);
                release(account);
                return None;
            }
            keys.push(kSecValueData as *const u8);
            values.push(data);
        }
        if returning_data {
            keys.push(kSecReturnData as *const u8);
            values.push(kCFBooleanTrue);
            keys.push(kSecMatchLimit as *const u8);
            values.push(kSecMatchLimitOne);
        }
        let dictionary = CFDictionaryCreate(
            ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            keys.len() as isize,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        );
        release(service);
        release(account);
        if !data.is_null() {
            release(data);
        }
        if dictionary.is_null() {
            None
        } else {
            Some(Dictionary(dictionary))
        }
    }
}

fn value_only(secret: &[u8]) -> Option<Dictionary> {
    unsafe {
        let data = CFDataCreate(ptr::null(), secret.as_ptr(), secret.len() as isize);
        if data.is_null() {
            return None;
        }
        let keys = [kSecValueData as *const u8];
        let values = [data];
        let dictionary = CFDictionaryCreate(
            ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        );
        release(data);
        if dictionary.is_null() {
            None
        } else {
            Some(Dictionary(dictionary))
        }
    }
}

fn cf_string(text: &str) -> *const u8 {
    unsafe {
        CFStringCreateWithBytes(ptr::null(), text.as_ptr(), text.len() as isize, 0x0800_0100, 0)
    }
}

fn release(value: *const u8) {
    if !value.is_null() {
        unsafe { CFRelease(value) };
    }
}

#[repr(C)]
struct DictionaryCallBacks {
    version: isize,
    retain: Option<extern "C" fn(*const u8, *const u8) -> *const u8>,
    release: Option<extern "C" fn(*const u8, *const u8)>,
    copy_description: Option<extern "C" fn(*const u8) -> *const u8>,
    equal: Option<extern "C" fn(*const u8, *const u8) -> u8>,
    hash: Option<extern "C" fn(*const u8) -> usize>,
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFBooleanTrue: *const u8;
    static kCFTypeDictionaryKeyCallBacks: DictionaryCallBacks;
    static kCFTypeDictionaryValueCallBacks: DictionaryCallBacks;

    fn CFStringCreateWithBytes(
        allocator: *const u8,
        bytes: *const u8,
        num_bytes: isize,
        encoding: u32,
        is_external: u8,
    ) -> *const u8;
    fn CFDataCreate(allocator: *const u8, bytes: *const u8, length: isize) -> *const u8;
    fn CFDataGetBytePtr(data: *const u8) -> *const u8;
    fn CFDataGetLength(data: *const u8) -> isize;
    fn CFDictionaryCreate(
        allocator: *const u8,
        keys: *const *const u8,
        values: *const *const u8,
        count: isize,
        key_callbacks: *const DictionaryCallBacks,
        value_callbacks: *const DictionaryCallBacks,
    ) -> *const u8;
    fn CFRelease(value: *const u8);
}

#[link(name = "Security", kind = "framework")]
extern "C" {
    static kSecClass: *const u8;
    static kSecClassGenericPassword: *const u8;
    static kSecAttrService: *const u8;
    static kSecAttrAccount: *const u8;
    static kSecValueData: *const u8;
    static kSecReturnData: *const u8;
    static kSecMatchLimit: *const u8;
    static kSecMatchLimitOne: *const u8;
    static kSecAttrAccessible: *const u8;
    static kSecAttrAccessibleWhenUnlockedThisDeviceOnly: *const u8;

    fn SecItemAdd(attributes: *const u8, result: *mut *const u8) -> i32;
    fn SecItemCopyMatching(query: *const u8, result: *mut *const u8) -> i32;
    fn SecItemUpdate(query: *const u8, attributes: *const u8) -> i32;
    fn SecItemDelete(query: *const u8) -> i32;
}

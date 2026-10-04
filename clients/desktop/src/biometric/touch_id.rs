//! Touch ID gate for the Mac login keychain. Compiled on macOS only.

use std::ptr;

use objc2::runtime::{AnyObject, Bool};
use objc2::{class, msg_send};
use zeroize::Zeroizing;

use super::BiometricError;

const SERVICE: &str = "com.kiefermenard.drop.mac";
const ACCOUNT: &str = "unlock";
const ERR_SUCCESS: i32 = 0;
const ERR_USER_CANCELED: i32 = -128;
const ERR_DUPLICATE_ITEM: i32 = -25299;
const ERR_ITEM_NOT_FOUND: i32 = -25300;
const ERR_INTERACTION_NOT_ALLOWED: i32 = -25308;
const ERR_AUTH_FAILED: i32 = -25293;
const BIOMETRY_CURRENT_SET: isize = 1 << 3;

pub fn label() -> &'static str {
    "Unlock with Touch ID"
}

pub fn available() -> bool {
    unsafe {
        let context: *mut AnyObject = msg_send![class!(LAContext), new];
        if context.is_null() {
            return false;
        }
        let mut error: *mut AnyObject = ptr::null_mut();
        let ok: Bool = msg_send![context, canEvaluatePolicy: 1isize, error: &mut error];
        if !error.is_null() {
            let _: () = msg_send![error, release];
        }
        let _: () = msg_send![context, release];
        ok.as_bool()
    }
}

pub fn enrolled() -> bool {
    let query = match query(false, true) {
        Some(query) => query,
        None => return false,
    };
    let status = unsafe { SecItemCopyMatching(query.0, ptr::null_mut()) };
    status == ERR_SUCCESS || status == ERR_INTERACTION_NOT_ALLOWED
}

pub fn store(secret: &[u8]) -> Result<(), BiometricError> {
    if !available() {
        return Err(BiometricError::Failed("Touch ID isn't available on this Mac.".into()));
    }
    let access = unsafe {
        SecAccessControlCreateWithFlags(
            ptr::null(),
            kSecAttrAccessibleWhenUnlockedThisDeviceOnly,
            BIOMETRY_CURRENT_SET,
            ptr::null_mut(),
        )
    };
    if access.is_null() {
        return Err(BiometricError::Failed("Couldn't store Touch ID unlock.".into()));
    }
    let dictionary = match add_dictionary(secret, access) {
        Some(dictionary) => dictionary,
        None => return Err(BiometricError::Failed("Couldn't store Touch ID unlock.".into())),
    };
    let status = unsafe { SecItemAdd(dictionary.0, ptr::null_mut()) };
    if status == ERR_SUCCESS {
        return Ok(());
    }
    if status == ERR_USER_CANCELED {
        return Err(BiometricError::Canceled);
    }
    // A canceled replacement must leave the previous item in the keychain.
    if status == ERR_DUPLICATE_ITEM {
        return update_secret(secret);
    }
    Err(BiometricError::Failed("Couldn't store Touch ID unlock.".into()))
}

fn update_secret(secret: &[u8]) -> Result<(), BiometricError> {
    let query = query(false, false).ok_or_else(|| BiometricError::Failed("Couldn't store Touch ID unlock.".into()))?;
    let attributes = match value_dictionary(secret) {
        Some(attributes) => attributes,
        None => return Err(BiometricError::Failed("Couldn't store Touch ID unlock.".into())),
    };
    let status = unsafe { SecItemUpdate(query.0, attributes.0) };
    if status == ERR_SUCCESS {
        return Ok(());
    }
    if status == ERR_USER_CANCELED {
        return Err(BiometricError::Canceled);
    }
    Err(BiometricError::Failed("Couldn't store Touch ID unlock.".into()))
}

pub fn load() -> Result<Zeroizing<Vec<u8>>, BiometricError> {
    let query = query(true, false).ok_or_else(|| BiometricError::Failed("Couldn't unlock with Touch ID.".into()))?;
    let mut result: CfType = ptr::null();
    let status = unsafe { SecItemCopyMatching(query.0, &mut result) };
    if status == ERR_USER_CANCELED {
        return Err(BiometricError::Canceled);
    }
    if status == ERR_ITEM_NOT_FOUND {
        return Err(BiometricError::Failed("Touch ID unlock is off.".into()));
    }
    if status == ERR_AUTH_FAILED || status != ERR_SUCCESS || result.is_null() {
        return Err(BiometricError::Failed(
            "Touch ID didn't unlock Drop. Enter your password.".into(),
        ));
    }
    let data = unsafe {
        let len = CFDataGetLength(result);
        let ptr_bytes = CFDataGetBytePtr(result);
        let mut bytes = Zeroizing::new(vec![0u8; len.max(0) as usize]);
        if len > 0 && !ptr_bytes.is_null() {
            ptr::copy_nonoverlapping(ptr_bytes, bytes.as_mut_ptr(), len as usize);
        }
        CFRelease(result);
        bytes
    };
    if data.is_empty() {
        return Err(BiometricError::Failed(
            "Touch ID didn't unlock Drop. Enter your password.".into(),
        ));
    }
    Ok(data)
}

pub fn delete() {
    if let Some(query) = query(false, false) {
        unsafe { SecItemDelete(query.0) };
    }
}

struct Dictionary(CfDictionary);

impl Drop for Dictionary {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) };
    }
}

fn query(returning_data: bool, fail_ui: bool) -> Option<Dictionary> {
    unsafe {
        let service = cf_string(SERVICE);
        let account = cf_string(ACCOUNT);
        if service.is_null() || account.is_null() {
            if !service.is_null() {
                CFRelease(service);
            }
            if !account.is_null() {
                CFRelease(account);
            }
            return None;
        }
        let mut keys = vec![
            kSecClass as CfType,
            kSecAttrService as CfType,
            kSecAttrAccount as CfType,
        ];
        let mut values = vec![
            kSecClassGenericPassword as CfType,
            service,
            account,
        ];
        let prompt = if returning_data { cf_string("Unlock Drop") } else { ptr::null() };
        if returning_data {
            keys.push(kSecReturnData as CfType);
            values.push(kCFBooleanTrue);
            keys.push(kSecMatchLimit as CfType);
            values.push(kSecMatchLimitOne as CfType);
            if !prompt.is_null() {
                keys.push(kSecUseOperationPrompt as CfType);
                values.push(prompt);
            }
        }
        if fail_ui {
            keys.push(kSecUseAuthenticationUI as CfType);
            values.push(kSecUseAuthenticationUIFail as CfType);
        }
        let dictionary = CFDictionaryCreate(
            ptr::null(),
            keys.as_ptr() as *const *const u8,
            values.as_ptr() as *const *const u8,
            keys.len() as isize,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        );
        CFRelease(service);
        CFRelease(account);
        if !prompt.is_null() {
            CFRelease(prompt);
        }
        if dictionary.is_null() {
            None
        } else {
            Some(Dictionary(dictionary))
        }
    }
}

fn value_dictionary(secret: &[u8]) -> Option<Dictionary> {
    unsafe {
        let data = CFDataCreate(ptr::null(), secret.as_ptr(), secret.len() as isize);
        if data.is_null() {
            return None;
        }
        let keys = [kSecValueData as CfType];
        let values = [data];
        let dictionary = CFDictionaryCreate(
            ptr::null(),
            keys.as_ptr() as *const *const u8,
            values.as_ptr() as *const *const u8,
            keys.len() as isize,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        );
        CFRelease(data);
        if dictionary.is_null() {
            None
        } else {
            Some(Dictionary(dictionary))
        }
    }
}

fn add_dictionary(secret: &[u8], access: SecAccessControl) -> Option<Dictionary> {
    unsafe {
        let service = cf_string(SERVICE);
        let account = cf_string(ACCOUNT);
        if service.is_null() || account.is_null() {
            if !service.is_null() {
                CFRelease(service);
            }
            if !account.is_null() {
                CFRelease(account);
            }
            CFRelease(access as CfType);
            return None;
        }
        let data = CFDataCreate(ptr::null(), secret.as_ptr(), secret.len() as isize);
        if data.is_null() {
            CFRelease(service);
            CFRelease(account);
            CFRelease(access as CfType);
            return None;
        }
        let keys = [
            kSecClass as CfType,
            kSecAttrService as CfType,
            kSecAttrAccount as CfType,
            kSecValueData as CfType,
            kSecAttrAccessControl as CfType,
        ];
        let values = [
            kSecClassGenericPassword as CfType,
            service,
            account,
            data,
            access as CfType,
        ];
        let dictionary = CFDictionaryCreate(
            ptr::null(),
            keys.as_ptr() as *const *const u8,
            values.as_ptr() as *const *const u8,
            keys.len() as isize,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        );
        CFRelease(service);
        CFRelease(account);
        CFRelease(data);
        CFRelease(access as CfType);
        if dictionary.is_null() {
            None
        } else {
            Some(Dictionary(dictionary))
        }
    }
}

fn cf_string(text: &str) -> CfString {
    unsafe {
        CFStringCreateWithBytes(
            ptr::null(),
            text.as_ptr(),
            text.len() as isize,
            0x0800_0100,
            0,
        )
    }
}

type CfType = *const u8;
type CfString = *const u8;
type CfDictionary = *const u8;
type CfData = *const u8;
type SecAccessControl = *const u8;

#[repr(C)]
struct DictionaryCallBacks {
    version: isize,
    retain: Option<extern "C" fn(*const u8, *const u8) -> *const u8>,
    release: Option<extern "C" fn(*const u8, *const u8)>,
    copy_description: Option<extern "C" fn(*const u8) -> CfString>,
    equal: Option<extern "C" fn(*const u8, *const u8) -> u8>,
    hash: Option<extern "C" fn(*const u8) -> usize>,
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFBooleanTrue: CfType;
    static kCFTypeDictionaryKeyCallBacks: DictionaryCallBacks;
    static kCFTypeDictionaryValueCallBacks: DictionaryCallBacks;

    fn CFStringCreateWithBytes(
        allocator: *const u8,
        bytes: *const u8,
        num_bytes: isize,
        encoding: u32,
        is_external: u8,
    ) -> CfString;
    fn CFDataCreate(allocator: *const u8, bytes: *const u8, length: isize) -> CfData;
    fn CFDataGetBytePtr(data: CfData) -> *const u8;
    fn CFDataGetLength(data: CfData) -> isize;
    fn CFDictionaryCreate(
        allocator: *const u8,
        keys: *const *const u8,
        values: *const *const u8,
        count: isize,
        key_callbacks: *const DictionaryCallBacks,
        value_callbacks: *const DictionaryCallBacks,
    ) -> CfDictionary;
    fn CFRelease(value: CfType);
}

#[link(name = "Security", kind = "framework")]
extern "C" {
    static kSecClass: CfString;
    static kSecClassGenericPassword: CfString;
    static kSecAttrService: CfString;
    static kSecAttrAccount: CfString;
    static kSecValueData: CfString;
    static kSecAttrAccessControl: CfString;
    static kSecReturnData: CfString;
    static kSecMatchLimit: CfString;
    static kSecMatchLimitOne: CfString;
    static kSecUseOperationPrompt: CfString;
    static kSecUseAuthenticationUI: CfString;
    static kSecUseAuthenticationUIFail: CfString;
    static kSecAttrAccessibleWhenUnlockedThisDeviceOnly: CfString;

    fn SecAccessControlCreateWithFlags(
        allocator: *const u8,
        protection: CfType,
        flags: isize,
        error: *mut *const u8,
    ) -> SecAccessControl;
    fn SecItemAdd(attributes: CfDictionary, result: *mut CfType) -> i32;
    fn SecItemCopyMatching(query: CfDictionary, result: *mut CfType) -> i32;
    fn SecItemUpdate(query: CfDictionary, attributes: CfDictionary) -> i32;
    fn SecItemDelete(query: CfDictionary) -> i32;
}

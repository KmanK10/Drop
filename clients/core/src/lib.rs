//! Client half of Drop.
//!
//! The content key and password stay in process memory. Nothing in this crate
//! writes either of them to disk. The session cookie lives in an in-memory jar
//! and disappears when the client is dropped.

mod bytes;
mod client;
mod crypto;
mod error;
mod format;
mod item;
mod kind;
mod settings;

pub use bytes::{b64url_to_bytes, bytes_to_b64url};
pub use client::{Account, CopyPayload, Downloaded, DropClient, ItemSummary, Snapshot};
pub use crypto::{
    account_material, assert_strong_kdf, decrypt, derive_keys, encrypt, registration_body,
    verify_key_check, AccountMaterial, Derived, KdfParams, KDF_MEMORY, KDF_PARALLELISM, KDF_TIME,
};
pub use error::DropError;
pub use format::{format_bytes, format_when, retention_label};
pub use item::{decode_item, encode_item, safe_download_name, ItemKind, ItemPlain};
pub use kind::{clipboard_file_kind, ClipboardKind};
pub use settings::{default_config_dir, load_settings, save_settings, Settings, DEFAULT_SERVER};

pub const KEY_CHECK_TEXT: &str = crypto::KEY_CHECK_TEXT;

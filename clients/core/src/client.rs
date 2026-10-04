use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::cookie::CookieStore;
use reqwest::header::{HeaderValue, ACCEPT, CONTENT_TYPE};
use reqwest::Url;
use serde_json::Value;
use zeroize::{Zeroize, Zeroizing};

use crate::bytes::{b64url_to_bytes, wipe_string};
use crate::crypto::{self, verify_key_check, KdfParams};
use crate::error::DropError;
use crate::format::{self, format_bytes, format_when, text_preview};
use crate::item::{self, safe_download_name, ItemKind, ItemPlain};
use crate::kind::{self, ClipboardKind};
use crate::unlock::UnlockMaterial;

const CSRF: &str = "x-drop-request";
const SESSION_COOKIE: &str = "drop_session";
const TTL_DEFAULT_MS: u64 = 30 * 24 * 60 * 60 * 1000;

#[derive(Debug, Clone)]
pub struct Account {
    pub username: String,
    pub role: String,
    pub quota_bytes: u64,
    pub used_bytes: u64,
    pub ttl_ms: u64,
}

#[derive(Debug, Clone)]
pub struct ItemSummary {
    pub id: String,
    pub created_at: i64,
    pub size: u64,
    pub kind: String,
    pub title: String,
    pub detail: String,
    pub when: String,
    pub can_copy: bool,
    pub broken: bool,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub account: Account,
    pub items: Vec<ItemSummary>,
    pub http: bool,
}

pub enum CopyPayload {
    Text(Zeroizing<String>),
    Image { mime: String, bytes: Zeroizing<Vec<u8>> },
}

pub struct Downloaded {
    pub name: String,
    pub bytes: Zeroizing<Vec<u8>>,
}

struct MemoryItem {
    summary: ItemSummary,
    text: Option<Zeroizing<String>>,
}

struct StoredCookie {
    name: String,
    value: String,
    host: String,
    path: String,
    secure: bool,
}

#[derive(Default)]
struct MemoryJar {
    cookies: Mutex<Vec<StoredCookie>>,
}

impl MemoryJar {
    fn clear(&self) {
        let mut guard = self.cookies.lock().expect("cookie jar");
        for cookie in guard.iter_mut() {
            wipe_string(&mut cookie.value);
        }
        guard.clear();
    }

    fn session_cookie(&self, host: &str) -> Option<Zeroizing<String>> {
        let guard = self.cookies.lock().expect("cookie jar");
        guard
            .iter()
            .find(|cookie| cookie.host == host && cookie.name == SESSION_COOKIE && !cookie.value.is_empty())
            .map(|cookie| Zeroizing::new(cookie.value.clone()))
    }

    fn install_session_cookie(&self, host: &str, value: &str, secure: bool) {
        let mut guard = self.cookies.lock().expect("cookie jar");
        for cookie in guard.iter_mut() {
            if cookie.name == SESSION_COOKIE && cookie.host == host {
                wipe_string(&mut cookie.value);
            }
        }
        guard.retain(|cookie| !(cookie.name == SESSION_COOKIE && cookie.host == host));
        guard.push(StoredCookie {
            name: SESSION_COOKIE.to_string(),
            value: value.to_string(),
            host: host.to_string(),
            path: "/".into(),
            secure,
        });
    }
}

impl CookieStore for MemoryJar {
    fn set_cookies(&self, cookie_headers: &mut dyn Iterator<Item = &HeaderValue>, url: &Url) {
        let Some(host) = url.host_str() else {
            return;
        };
        let mut guard = self.cookies.lock().expect("cookie jar");
        for header in cookie_headers {
            let Ok(text) = header.to_str() else {
                continue;
            };
            let Some(parsed) = parse_set_cookie(text, host) else {
                continue;
            };
            guard.retain(|cookie| !(cookie.name == parsed.name && cookie.host == parsed.host));
            if parsed.value.is_empty() {
                continue;
            }
            guard.push(parsed);
        }
    }

    fn cookies(&self, url: &Url) -> Option<HeaderValue> {
        let host = url.host_str()?;
        let https = url.scheme() == "https";
        let path = url.path();
        let guard = self.cookies.lock().expect("cookie jar");
        let mut parts = Vec::new();
        for cookie in guard.iter() {
            if cookie.host != host {
                continue;
            }
            if cookie.secure && !https {
                continue;
            }
            if !path_matches(&cookie.path, path) {
                continue;
            }
            parts.push(format!("{}={}", cookie.name, cookie.value));
        }
        if parts.is_empty() {
            return None;
        }
        HeaderValue::from_str(&parts.join("; ")).ok()
    }
}

pub struct DropClient {
    base: Url,
    http: reqwest::blocking::Client,
    jar: Arc<MemoryJar>,
    content_key: Option<Zeroizing<[u8; 32]>>,
    account: Option<Account>,
    items: Vec<MemoryItem>,
}

impl std::fmt::Debug for DropClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DropClient")
            .field("server", &self.base.as_str())
            .field("unlocked", &self.content_key.is_some())
            .finish()
    }
}

impl Drop for DropClient {
    fn drop(&mut self) {
        self.zero_local();
        self.jar.clear();
    }
}

impl DropClient {
    pub fn connect(server: &str) -> Result<Self, DropError> {
        let base = parse_server(server)?;
        let jar = Arc::new(MemoryJar::default());
        let http = reqwest::blocking::Client::builder()
            .cookie_provider(jar.clone())
            .timeout(Duration::from_secs(600))
            .connect_timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("Drop")
            .build()
            .map_err(|_| DropError::Network)?;
        Ok(Self {
            base,
            http,
            jar,
            content_key: None,
            account: None,
            items: Vec::new(),
        })
    }

    pub fn is_unlocked(&self) -> bool {
        self.content_key.is_some()
    }

    pub fn server_is_http(&self) -> bool {
        self.base.scheme() == "http"
    }

    pub fn sign_in(&mut self, username: &str, password: &str) -> Result<Snapshot, DropError> {
        let name = normalize_username(username).ok_or(DropError::BadUsername)?;
        if password.is_empty() {
            return Err(DropError::EmptyPassword);
        }
        let mut owned = Zeroizing::new(password.to_string());
        let result = self.sign_in_owned(&name, &owned);
        owned.zeroize();
        result
    }

    fn sign_in_owned(&mut self, username: &str, password: &str) -> Result<Snapshot, DropError> {
        self.zero_local();
        self.jar.clear();
        let (params, salt) = self.auth_params(username)?;
        let derived = crypto::derive_keys(password, &salt, &params)?;
        let login = self.post_json("/api/auth/login", &crypto::login_body(username, &derived.auth_verifier), false);
        if let Err(error) = login {
            return Err(error);
        }
        let me = match self.get_json("/api/me") {
            Ok(me) => me,
            Err(DropError::SignedOut) => return Err(DropError::SessionLost),
            Err(error) => {
                let _ = self.logout_request();
                self.jar.clear();
                return Err(error);
            }
        };
        let (me_params, me_salt) = match me.get("kdf") {
            Some(kdf) => crypto::parse_kdf(kdf)?,
            None => return Err(DropError::KeyCheck),
        };
        if me_params.algo != params.algo
            || me_params.memory != params.memory
            || me_params.time != params.time
            || me_params.parallelism != params.parallelism
            || me_salt != salt
        {
            let _ = self.logout_request();
            self.jar.clear();
            return Err(DropError::KeyCheck);
        }
        let key_check = me
            .get("keyCheck")
            .and_then(|v| v.as_str())
            .ok_or(DropError::KeyCheck)?;
        let key_check = b64url_to_bytes(key_check).map_err(|_| DropError::KeyCheck)?;
        if !verify_key_check(&derived.content_key, &key_check).unwrap_or(false) {
            let _ = self.logout_request();
            self.jar.clear();
            return Err(DropError::KeyCheck);
        }
        let ttl_ms = self.fetch_ttl();
        self.content_key = Some(derived.content_key);
        self.account = Some(account_from_me(&me, ttl_ms)?);
        self.refresh()
    }

    pub fn sign_out(&mut self) {
        self.zero_local();
        let _ = self.logout_request();
        self.jar.clear();
    }

    /// The content key and session cookie currently in memory. The password is
    /// not included. The caller stores this only in a biometric keychain.
    pub fn unlock_material(&self) -> Result<UnlockMaterial, DropError> {
        let content_key = self.content_key.clone().ok_or(DropError::Locked)?;
        let username = self
            .account
            .as_ref()
            .map(|account| account.username.clone())
            .filter(|name| !name.is_empty())
            .ok_or(DropError::Locked)?;
        let host = self.base.host_str().ok_or(DropError::BadServer)?;
        let cookie = self.jar.session_cookie(host).ok_or(DropError::SessionLost)?;
        Ok(UnlockMaterial {
            content_key,
            server_url: trim_server(self.base.as_str()),
            username,
            cookie,
        })
    }

    /// Install a biometric unlock blob and load the clipboard. A mismatch or a
    /// dead session leaves the client locked so the password form can be used.
    pub fn restore(&mut self, mut material: UnlockMaterial) -> Result<Snapshot, DropError> {
        if trim_server(&material.server_url) != trim_server(self.base.as_str()) {
            return Err(DropError::Message("Enter your password.".into()));
        }
        self.zero_local();
        self.jar.clear();
        let host = self.base.host_str().unwrap_or("").to_string();
        if host.is_empty() {
            return Err(DropError::BadServer);
        }
        self.jar
            .install_session_cookie(&host, material.cookie.as_str(), self.base.scheme() == "https");
        material.cookie.zeroize();
        let me = match self.get_json("/api/me") {
            Ok(me) => me,
            Err(error) => {
                self.jar.clear();
                return Err(error);
            }
        };
        let key_check = match me
            .get("keyCheck")
            .and_then(|value| value.as_str())
            .ok_or(DropError::KeyCheck)
            .and_then(|text| b64url_to_bytes(text).map_err(|_| DropError::KeyCheck))
        {
            Ok(bytes) => bytes,
            Err(error) => {
                self.jar.clear();
                return Err(error);
            }
        };
        if !verify_key_check(&material.content_key, &key_check).unwrap_or(false) {
            self.jar.clear();
            return Err(DropError::KeyCheck);
        }
        let account_name = me.get("username").and_then(|value| value.as_str()).unwrap_or("");
        if account_name != material.username {
            self.jar.clear();
            return Err(DropError::Message("Enter your password.".into()));
        }
        let ttl_ms = self.fetch_ttl();
        self.content_key = Some(std::mem::replace(
            &mut material.content_key,
            Zeroizing::new([0u8; 32]),
        ));
        self.account = Some(account_from_me(&me, ttl_ms)?);
        match self.refresh() {
            Ok(snapshot) => Ok(snapshot),
            Err(error) => {
                self.zero_local();
                self.jar.clear();
                Err(error)
            }
        }
    }

    pub fn refresh(&mut self) -> Result<Snapshot, DropError> {
        self.ensure_unlocked()?;
        let payload = match self.get_json("/api/items") {
            Ok(payload) => payload,
            Err(DropError::SignedOut) => {
                self.zero_local();
                self.jar.clear();
                return Err(DropError::SignedOut);
            }
            Err(error) => return Err(error),
        };
        let used = json_u64(payload.get("usedBytes")).unwrap_or(0);
        let quota = json_u64(payload.get("quotaBytes")).unwrap_or(0);
        if let Some(account) = self.account.as_mut() {
            account.used_bytes = used;
            if quota > 0 {
                account.quota_bytes = quota;
            }
        }
        let rows = payload.get("items").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let mut next = Vec::with_capacity(rows.len());
        for row in rows {
            let id = row.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if !valid_id(&id) {
                continue;
            }
            let created_at = row.get("createdAt").and_then(|v| v.as_i64()).unwrap_or(0);
            let size = json_u64(row.get("size")).unwrap_or(0);
            if let Some(existing) = self.items.iter().find(|item| item.summary.id == id) {
                let mut summary = existing.summary.clone();
                summary.created_at = created_at;
                summary.size = size;
                summary.when = format_when(created_at, format::now_ms());
                summary.detail = detail_line(&summary, size);
                next.push(MemoryItem {
                    summary,
                    text: existing.text.clone(),
                });
                continue;
            }
            match self.decrypt_new(&id, created_at, size) {
                Ok(item) => next.push(item),
                Err(DropError::SignedOut) => {
                    self.zero_local();
                    self.jar.clear();
                    return Err(DropError::SignedOut);
                }
                Err(DropError::Missing) => continue,
                Err(_) => next.push(broken_item(&id, created_at, size)),
            }
        }
        self.items = next;
        self.snapshot()
    }

    pub fn upload_text(&mut self, text: &str) -> Result<Snapshot, DropError> {
        if text.trim().is_empty() {
            return Err(DropError::EmptyText);
        }
        let body = text.as_bytes().to_vec();
        self.upload_plain(ItemPlain {
            kind: ItemKind::Text,
            name: String::new(),
            mime: "text/plain".into(),
            body,
        })
    }

    pub fn upload_file(&mut self, name: &str, mime: &str, bytes: &[u8]) -> Result<Snapshot, DropError> {
        let name = if name.trim().is_empty() { "file".into() } else { name.to_string() };
        let mime = if mime.trim().is_empty() {
            "application/octet-stream".into()
        } else {
            mime.to_string()
        };
        self.upload_plain(ItemPlain {
            kind: ItemKind::File,
            name,
            mime,
            body: bytes.to_vec(),
        })
    }

    pub fn delete_item(&mut self, id: &str) -> Result<Snapshot, DropError> {
        self.ensure_unlocked()?;
        let path = item_path(id)?;
        let (status, bytes) = self.request(reqwest::Method::DELETE, &path, None, None)?;
        if status == 401 {
            self.zero_local();
            self.jar.clear();
            return Err(DropError::SignedOut);
        }
        if !(200..300).contains(&status) {
            return Err(error_from(status, &bytes));
        }
        self.items.retain(|item| item.summary.id != id);
        self.refresh()
    }

    pub fn copy_item(&mut self, id: &str) -> Result<CopyPayload, DropError> {
        self.ensure_unlocked()?;
        if let Some(item) = self.items.iter().find(|item| item.summary.id == id) {
            if item.summary.broken {
                return Err(DropError::CantCopy);
            }
            if item.summary.kind == "text" {
                let text = item.text.clone().unwrap_or_else(|| Zeroizing::new(String::new()));
                return Ok(CopyPayload::Text(text));
            }
        }
        let plain = self.fetch_plain(id)?;
        match plain.kind {
            ItemKind::Text => Ok(CopyPayload::Text(Zeroizing::new(
                String::from_utf8_lossy(&plain.body).into_owned(),
            ))),
            ItemKind::File => match kind::clipboard_file_kind(&plain.mime) {
                Some(ClipboardKind::Text) | Some(ClipboardKind::Html) => Ok(CopyPayload::Text(Zeroizing::new(
                    String::from_utf8_lossy(&plain.body).into_owned(),
                ))),
                Some(ClipboardKind::Image) => Ok(CopyPayload::Image {
                    mime: kind::base_mime(&plain.mime),
                    bytes: Zeroizing::new(plain.body),
                }),
                None => Err(DropError::CantCopy),
            },
        }
    }

    pub fn download_item(&mut self, id: &str) -> Result<Downloaded, DropError> {
        self.ensure_unlocked()?;
        let plain = self.fetch_plain(id)?;
        let name = match plain.kind {
            ItemKind::Text => {
                let named = safe_download_name(&plain.name);
                if named == "download" {
                    "drop.txt".into()
                } else if named.contains('.') {
                    named
                } else {
                    format!("{named}.txt")
                }
            }
            ItemKind::File => safe_download_name(&plain.name),
        };
        Ok(Downloaded {
            name,
            bytes: Zeroizing::new(plain.body),
        })
    }

    fn upload_plain(&mut self, plain: ItemPlain) -> Result<Snapshot, DropError> {
        let key = self.ensure_unlocked()?.clone();
        let account = self.account.clone().ok_or(DropError::Locked)?;
        let mut plain = plain;
        let encoded = Zeroizing::new(item::encode_item(&plain)?);
        for byte in &mut plain.body {
            *byte = 0;
        }
        let overhead = encoded.len() as u64 + 32;
        if account.used_bytes.saturating_add(overhead) > account.quota_bytes {
            return Err(DropError::Server(format!(
                "That item doesn't fit. {} of {} is already used.",
                format_bytes(account.used_bytes),
                format_bytes(account.quota_bytes)
            )));
        }
        let ciphertext = crypto::encrypt(&key, &encoded)?;
        drop(encoded);
        let (status, bytes) = self.request(
            reqwest::Method::POST,
            "/api/items",
            Some(ciphertext),
            Some("application/octet-stream"),
        )?;
        if status == 401 {
            self.zero_local();
            self.jar.clear();
            return Err(DropError::SignedOut);
        }
        if !(200..300).contains(&status) {
            return Err(error_from(status, &bytes));
        }
        self.refresh()
    }

    fn decrypt_new(&self, id: &str, created_at: i64, size: u64) -> Result<MemoryItem, DropError> {
        let plain = self.fetch_plain(id)?;
        let now = format::now_ms();
        let kind = plain.kind.as_str().to_string();
        let (title, detail, can_copy, text) = consume_plain(plain, size);
        Ok(MemoryItem {
            summary: ItemSummary {
                id: id.to_string(),
                created_at,
                size,
                kind,
                title,
                detail,
                when: format_when(created_at, now),
                can_copy,
                broken: false,
            },
            text,
        })
    }

    fn fetch_plain(&self, id: &str) -> Result<ItemPlain, DropError> {
        let key = self.content_key.as_ref().ok_or(DropError::Locked)?;
        let path = item_path(id)?;
        let (status, bytes) = self.request(reqwest::Method::GET, &path, None, None)?;
        if status == 401 {
            return Err(DropError::SignedOut);
        }
        if status == 404 {
            return Err(DropError::Missing);
        }
        if !(200..300).contains(&status) {
            return Err(error_from(status, &bytes));
        }
        let mut plain_bytes = Zeroizing::new(crypto::decrypt(key, &bytes)?);
        let item = item::decode_item(&plain_bytes)?;
        plain_bytes.zeroize();
        Ok(item)
    }

    fn auth_params(&self, username: &str) -> Result<(KdfParams, Vec<u8>), DropError> {
        let body = self.post_json("/api/auth/params", &serde_json::json!({ "username": username }), false)?;
        crypto::parse_kdf(&body)
    }

    fn fetch_ttl(&self) -> u64 {
        self.get_json("/api/meta")
            .ok()
            .and_then(|meta| json_u64(meta.get("itemTtlMs")))
            .filter(|ttl| *ttl > 0)
            .unwrap_or(TTL_DEFAULT_MS)
    }

    fn logout_request(&self) -> Result<(), DropError> {
        let url = self.url("/api/auth/logout")?;
        let _ = self
            .http
            .post(url)
            .header(CSRF, "1")
            .timeout(Duration::from_secs(3))
            .send();
        Ok(())
    }

    fn snapshot(&self) -> Result<Snapshot, DropError> {
        let account = self.account.clone().ok_or(DropError::Locked)?;
        Ok(Snapshot {
            account,
            items: self.items.iter().map(|item| item.summary.clone()).collect(),
            http: self.server_is_http(),
        })
    }

    fn ensure_unlocked(&self) -> Result<&Zeroizing<[u8; 32]>, DropError> {
        self.content_key.as_ref().ok_or(DropError::Locked)
    }

    fn zero_local(&mut self) {
        if let Some(key) = self.content_key.as_mut() {
            key.zeroize();
        }
        self.content_key = None;
        for item in &mut self.items {
            if let Some(text) = item.text.as_mut() {
                text.zeroize();
            }
        }
        self.items.clear();
        self.account = None;
    }

    fn post_json(&self, path: &str, body: &Value, session: bool) -> Result<Value, DropError> {
        let bytes = serde_json::to_vec(body).map_err(|_| DropError::Message("Couldn't build that request.".into()))?;
        let (status, response) = self.request(
            reqwest::Method::POST,
            path,
            Some(bytes),
            Some("application/json"),
        )?;
        interpret_json(status, &response, session)
    }

    fn get_json(&self, path: &str) -> Result<Value, DropError> {
        let (status, response) = self.request(reqwest::Method::GET, path, None, None)?;
        interpret_json(status, &response, true)
    }

    fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Vec<u8>>,
        content_type: Option<&str>,
    ) -> Result<(u16, Vec<u8>), DropError> {
        let url = self.url(path)?;
        let mut builder = self.http.request(method.clone(), url).header(ACCEPT, "application/json");
        if method != reqwest::Method::GET && method != reqwest::Method::HEAD {
            builder = builder.header(CSRF, "1");
        }
        if let Some(content_type) = content_type {
            builder = builder.header(CONTENT_TYPE, content_type);
        }
        if let Some(body) = body {
            builder = builder.body(body);
        }
        let response = builder.send().map_err(|_| DropError::Network)?;
        let status = response.status().as_u16();
        let bytes = response.bytes().map_err(|_| DropError::Network)?.to_vec();
        Ok((status, bytes))
    }

    fn url(&self, path: &str) -> Result<Url, DropError> {
        let path = path.trim_start_matches('/');
        self.base.join(path).map_err(|_| DropError::BadServer)
    }
}

fn trim_server(input: &str) -> String {
    input.trim().trim_end_matches('/').to_string()
}

pub fn parse_server(input: &str) -> Result<Url, DropError> {
    let trimmed = input.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(DropError::BadServer);
    }
    let url = Url::parse(trimmed).map_err(|_| DropError::BadServer)?;
    if url.scheme() != "https" && url.scheme() != "http" {
        return Err(DropError::BadServer);
    }
    if url.host_str().is_none() {
        return Err(DropError::BadServer);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(DropError::Message(
            "Don't put a username or password in the server address.".into(),
        ));
    }
    if url.path() != "/" && !url.path().is_empty() {
        return Err(DropError::Message("The server address shouldn't include a path.".into()));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(DropError::BadServer);
    }
    Ok(url)
}

pub fn normalize_username(value: &str) -> Option<String> {
    let name: String = value.trim().nfkc_lower();
    let mut chars = name.chars();
    let first = chars.next()?;
    if !first.is_ascii_lowercase() {
        return None;
    }
    if !(2..=32).contains(&name.len()) {
        return None;
    }
    if !name
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')
    {
        return None;
    }
    Some(name)
}

trait NfkcLower {
    fn nfkc_lower(&self) -> String;
}

impl NfkcLower for str {
    fn nfkc_lower(&self) -> String {
        unicode_normalization::UnicodeNormalization::nfkc(self)
            .collect::<String>()
            .to_lowercase()
    }
}

fn account_from_me(me: &Value, ttl_ms: u64) -> Result<Account, DropError> {
    Ok(Account {
        username: me.get("username").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        role: me.get("role").and_then(|v| v.as_str()).unwrap_or("user").to_string(),
        quota_bytes: json_u64(me.get("quotaBytes")).unwrap_or(0),
        used_bytes: json_u64(me.get("usedBytes")).unwrap_or(0),
        ttl_ms,
    })
}

fn consume_plain(mut plain: ItemPlain, size: u64) -> (String, String, bool, Option<Zeroizing<String>>) {
    let summarized = summarize(&plain, size);
    for byte in &mut plain.body {
        *byte = 0;
    }
    summarized
}

fn summarize(plain: &ItemPlain, size: u64) -> (String, String, bool, Option<Zeroizing<String>>) {
    match plain.kind {
        ItemKind::Text => {
            let text = String::from_utf8_lossy(&plain.body).into_owned();
            let title = {
                let preview = text_preview(&text, 80);
                if preview.is_empty() {
                    "Empty note".into()
                } else {
                    preview
                }
            };
            let detail = format!("Text · {}", format_bytes(text.len() as u64));
            (title, detail, true, Some(Zeroizing::new(text)))
        }
        ItemKind::File => {
            let title = if plain.name.trim().is_empty() {
                "Untitled file".into()
            } else {
                plain.name.clone()
            };
            let detail = format_bytes(size);
            let can_copy = kind::can_copy_file(&plain.mime);
            (title, detail, can_copy, None)
        }
    }
}

fn detail_line(summary: &ItemSummary, size: u64) -> String {
    if summary.kind == "text" {
        summary.detail.clone()
    } else if summary.broken {
        "Can't decrypt".into()
    } else {
        format_bytes(size)
    }
}

fn broken_item(id: &str, created_at: i64, size: u64) -> MemoryItem {
    MemoryItem {
        summary: ItemSummary {
            id: id.to_string(),
            created_at,
            size,
            kind: "file".into(),
            title: "Can't decrypt this item".into(),
            detail: format_bytes(size),
            when: format_when(created_at, format::now_ms()),
            can_copy: false,
            broken: true,
        },
        text: None,
    }
}

fn interpret_json(status: u16, bytes: &[u8], session: bool) -> Result<Value, DropError> {
    if session && status == 401 {
        return Err(DropError::SignedOut);
    }
    if !(200..300).contains(&status) {
        return Err(error_from(status, bytes));
    }
    serde_json::from_slice(bytes).map_err(|_| DropError::Message("Something went wrong.".into()))
}

fn error_from(status: u16, bytes: &[u8]) -> DropError {
    if let Ok(value) = serde_json::from_slice::<Value>(bytes) {
        if let Some(message) = value.get("error").and_then(|v| v.as_str()) {
            if !message.is_empty() && message.len() < 400 && !message.contains('\n') && !message.contains('\r') {
                return DropError::Server(message.to_string());
            }
        }
    }
    if status == 413 {
        return DropError::Server("That item is too large.".into());
    }
    DropError::Server("Something went wrong.".into())
}

fn json_u64(value: Option<&Value>) -> Option<u64> {
    value?.as_u64().or_else(|| value?.as_i64().and_then(|n| u64::try_from(n).ok()))
}

fn valid_id(id: &str) -> bool {
    id.len() == 36
        && id
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() || ch == '-')
}

fn item_path(id: &str) -> Result<String, DropError> {
    if !valid_id(id) {
        return Err(DropError::Missing);
    }
    Ok(format!("/api/items/{id}"))
}

fn parse_set_cookie(header: &str, host: &str) -> Option<StoredCookie> {
    let (pair, rest) = header.split_once(';').unwrap_or((header, ""));
    let (name, value) = pair.split_once('=')?;
    let name = name.trim();
    if name.is_empty() || name.starts_with('$') {
        return None;
    }
    let mut path = "/".to_string();
    let mut secure = false;
    let mut delete = false;
    for part in rest.split(';') {
        let part = part.trim();
        let (key, raw) = part.split_once('=').unwrap_or((part, ""));
        match key.trim().to_ascii_lowercase().as_str() {
            "secure" => secure = true,
            "path" => {
                let candidate = raw.trim();
                if candidate.starts_with('/') && candidate.len() < 200 {
                    path = candidate.to_string();
                }
            }
            "max-age" => {
                if raw.trim().parse::<i64>().unwrap_or(1) <= 0 {
                    delete = true;
                }
            }
            _ => {}
        }
    }
    Some(StoredCookie {
        name: name.to_string(),
        value: if delete { String::new() } else { value.trim().to_string() },
        host: host.to_string(),
        path,
        secure,
    })
}

fn path_matches(cookie_path: &str, request_path: &str) -> bool {
    if cookie_path == "/" {
        return true;
    }
    request_path.starts_with(cookie_path)
}

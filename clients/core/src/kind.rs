#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardKind {
    Text,
    Html,
    Image,
}

pub fn base_mime(mime: &str) -> String {
    mime.split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

fn is_text_mime(base: &str) -> bool {
    const TEXT_TYPES: &[&str] = &[
        "application/json",
        "application/xml",
        "application/javascript",
        "application/x-javascript",
        "application/ecmascript",
        "application/sql",
        "application/graphql",
        "application/yaml",
        "application/x-yaml",
        "application/toml",
        "application/x-toml",
        "application/x-sh",
        "application/csv",
        "image/svg+xml",
    ];
    if base.starts_with("text/") {
        return true;
    }
    if TEXT_TYPES.contains(&base) {
        return true;
    }
    base.starts_with("application/") && (base.ends_with("+json") || base.ends_with("+xml"))
}

/// What a decrypted file can become on the system clipboard.
pub fn clipboard_file_kind(mime: &str) -> Option<ClipboardKind> {
    let base = base_mime(mime);
    if base.is_empty() || base == "application/octet-stream" {
        return None;
    }
    if base == "text/html" || base == "application/xhtml+xml" {
        return Some(ClipboardKind::Html);
    }
    if is_text_mime(&base) {
        return Some(ClipboardKind::Text);
    }
    if base.starts_with("image/") {
        return Some(ClipboardKind::Image);
    }
    None
}

pub fn can_copy_file(mime: &str) -> bool {
    clipboard_file_kind(mime).is_some()
}

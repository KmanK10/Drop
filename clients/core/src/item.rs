use crate::error::DropError;

const MAGIC: [u8; 4] = [0x44, 0x52, 0x50, 0x31]; // DRP1

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Text,
    File,
}

impl ItemKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ItemKind::Text => "text",
            ItemKind::File => "file",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ItemPlain {
    pub kind: ItemKind,
    pub name: String,
    pub mime: String,
    pub body: Vec<u8>,
}

pub fn encode_item(item: &ItemPlain) -> Result<Vec<u8>, DropError> {
    let name = item.name.as_bytes();
    let mime = item.mime.as_bytes();
    if name.len() > 512 {
        return Err(DropError::NameTooLong);
    }
    if mime.len() > 200 {
        return Err(DropError::MimeTooLong);
    }
    let mut out = Vec::with_capacity(4 + 1 + 2 + name.len() + 2 + mime.len() + item.body.len());
    out.extend_from_slice(&MAGIC);
    out.push(match item.kind {
        ItemKind::Text => 1,
        ItemKind::File => 2,
    });
    out.extend_from_slice(&(name.len() as u16).to_be_bytes());
    out.extend_from_slice(name);
    out.extend_from_slice(&(mime.len() as u16).to_be_bytes());
    out.extend_from_slice(mime);
    out.extend_from_slice(&item.body);
    Ok(out)
}

pub fn decode_item(bytes: &[u8]) -> Result<ItemPlain, DropError> {
    if bytes.len() < 4 + 1 + 2 + 2 {
        return Err(DropError::TruncatedItem);
    }
    if bytes[0..4] != MAGIC {
        return Err(DropError::BadItem);
    }
    let kind = match bytes[4] {
        1 => ItemKind::Text,
        2 => ItemKind::File,
        _ => return Err(DropError::BadItem),
    };
    let name_len = u16::from_be_bytes([bytes[5], bytes[6]]) as usize;
    let mut offset = 7;
    if offset + name_len + 2 > bytes.len() {
        return Err(DropError::TruncatedItem);
    }
    let name = String::from_utf8_lossy(&bytes[offset..offset + name_len]).into_owned();
    offset += name_len;
    let mime_len = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]) as usize;
    offset += 2;
    if offset + mime_len > bytes.len() {
        return Err(DropError::TruncatedItem);
    }
    let mime = String::from_utf8_lossy(&bytes[offset..offset + mime_len]).into_owned();
    offset += mime_len;
    Ok(ItemPlain {
        kind,
        name,
        mime,
        body: bytes[offset..].to_vec(),
    })
}

pub fn safe_download_name(name: &str) -> String {
    let mut base = name.replace(['/', '\\'], "");
    while base.starts_with('.') {
        base = base[1..].to_string();
    }
    let base = base.trim();
    let mut chars = base.chars();
    let mut out = String::new();
    for ch in chars.by_ref().take(180) {
        out.push(ch);
    }
    let trimmed = out.trim().to_string();
    if trimmed.is_empty() {
        "download".into()
    } else {
        trimmed
    }
}

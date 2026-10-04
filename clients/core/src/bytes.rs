use sha2::{Digest, Sha256};

pub fn bytes_to_b64url(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8) | bytes[i + 2] as u32;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(TABLE[((n >> 6) & 63) as usize] as char);
        out.push(TABLE[(n & 63) as usize] as char);
        i += 3;
    }
    if i < bytes.len() {
        let mut n = (bytes[i] as u32) << 16;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        if i + 1 < bytes.len() {
            n |= (bytes[i + 1] as u32) << 8;
            out.push(TABLE[((n >> 12) & 63) as usize] as char);
            out.push(TABLE[((n >> 6) & 63) as usize] as char);
        } else {
            out.push(TABLE[((n >> 12) & 63) as usize] as char);
        }
    }
    out
}

pub fn b64url_to_bytes(value: &str) -> Result<Vec<u8>, &'static str> {
    if !value.bytes().all(|b| matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_')) {
        return Err("Invalid encoding.");
    }
    fn val(b: u8) -> u8 {
        match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'-' => 62,
            _ => 63,
        }
    }
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut i = 0;
    while i + 4 <= bytes.len() {
        let n = ((val(bytes[i]) as u32) << 18)
            | ((val(bytes[i + 1]) as u32) << 12)
            | ((val(bytes[i + 2]) as u32) << 6)
            | val(bytes[i + 3]) as u32;
        out.push((n >> 16) as u8);
        out.push((n >> 8) as u8);
        out.push(n as u8);
        i += 4;
    }
    let rest = bytes.len() - i;
    if rest == 1 {
        return Err("Invalid encoding.");
    }
    if rest >= 2 {
        let mut n = ((val(bytes[i]) as u32) << 18) | ((val(bytes[i + 1]) as u32) << 12);
        out.push((n >> 16) as u8);
        if rest == 3 {
            n |= (val(bytes[i + 2]) as u32) << 6;
            out.push((n >> 8) as u8);
        }
    }
    Ok(out)
}

pub fn sha256(data: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(data);
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

pub fn timing_equal(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

pub fn wipe_string(value: &mut String) {
    let mut taken = std::mem::take(value);
    // Safety: we only overwrite the existing buffer, then drop it empty of secrets.
    for byte in unsafe { taken.as_bytes_mut() } {
        *byte = 0;
    }
    taken.clear();
}

/// Green disc with a cream D, matching the browser wordmark.
pub fn tray_rgba(size: u32) -> Vec<u8> {
    disc(size, [0x1d, 0x68, 0x43, 0xff], [0xf4, 0xff, 0xf7, 0xff])
}

/// Black disc with a clear D, for the macOS template menu-bar image.
#[cfg(target_os = "macos")]
pub fn menu_bar_rgba(size: u32) -> Vec<u8> {
    disc(size, [0x00, 0x00, 0x00, 0xff], [0x00, 0x00, 0x00, 0x00])
}

fn disc(size: u32, fill: [u8; 4], letter: [u8; 4]) -> Vec<u8> {
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let center = (size as f32 - 1.0) / 2.0;
    let radius = center * 0.94;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let distance = (dx * dx + dy * dy).sqrt();
            if distance > radius + 0.8 {
                continue;
            }
            let coverage = (radius + 0.6 - distance).clamp(0.0, 1.0);
            let nx = dx / radius;
            let ny = dy / radius;
            let mut pixel = if in_letter_d(nx, ny) { letter } else { fill };
            pixel[3] = (pixel[3] as f32 * coverage) as u8;
            let index = ((y * size + x) * 4) as usize;
            rgba[index..index + 4].copy_from_slice(&pixel);
        }
    }
    rgba
}

fn in_letter_d(nx: f32, ny: f32) -> bool {
    if (-0.42..-0.16).contains(&nx) && (-0.48..0.48).contains(&ny) {
        return true;
    }
    let outer_x = (nx + 0.08) / 0.46;
    let outer_y = ny / 0.48;
    let outer = outer_x >= -0.05 && outer_x * outer_x + outer_y * outer_y <= 1.0;
    let inner_x = (nx + 0.02) / 0.24;
    let inner_y = ny / 0.28;
    let inner = inner_x * inner_x + inner_y * inner_y <= 1.0;
    outer && !inner
}

#[cfg(target_os = "macos")]
pub fn png_bytes(rgba: &[u8], size: u32) -> Vec<u8> {
    // A tiny uncompressed PNG so the menu bar can load pixels without the image crate.
    fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let mut crc_data = kind.to_vec();
        crc_data.extend_from_slice(data);
        out.extend_from_slice(&crc32(&crc_data).to_be_bytes());
        out
    }
    let mut raw = Vec::with_capacity((size as usize) * (1 + size as usize * 4));
    for row in 0..size {
        raw.push(0);
        let start = (row * size * 4) as usize;
        raw.extend_from_slice(&rgba[start..start + (size * 4) as usize]);
    }
    let mut image = Vec::new();
    image.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&size.to_be_bytes());
    ihdr.extend_from_slice(&size.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    image.extend(chunk(b"IHDR", &ihdr));
    image.extend(chunk(b"IDAT", &deflate_store(&raw)));
    image.extend(chunk(b"IEND", &[]));
    image
}

#[cfg(target_os = "macos")]
fn deflate_store(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    let mut index = 0;
    while index < data.len() {
        let take = (data.len() - index).min(65535);
        let last = index + take == data.len();
        out.push(if last { 1 } else { 0 });
        out.extend_from_slice(&(take as u16).to_le_bytes());
        out.extend_from_slice(&(!(take as u16)).to_le_bytes());
        out.extend_from_slice(&data[index..index + take]);
        index += take;
    }
    let mut s1: u32 = 1;
    let mut s2: u32 = 0;
    for byte in data {
        s1 = (s1 + *byte as u32) % 65521;
        s2 = (s2 + s1) % 65521;
    }
    out.extend_from_slice(&(((s2 << 16) | s1).to_be_bytes()));
    out
}

#[cfg(target_os = "macos")]
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in data {
        crc ^= *byte as u32;
        for _ in 0..8 {
            let mask = if crc & 1 == 1 { 0xedb8_8320 } else { 0 };
            crc = (crc >> 1) ^ mask;
        }
    }
    !crc
}

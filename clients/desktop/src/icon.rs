/// Green disc with a cream D, matching the browser wordmark.
pub fn tray_rgba(size: u32) -> Vec<u8> {
    disc(size, [0x1d, 0x68, 0x43, 0xff], [0xf4, 0xff, 0xf7, 0xff])
}

/// The app-icon clipboard, as a menu-bar template.
///
/// The page, clip, clip hole, and three rules use the same boxes as
/// `desktop/make-icon.py`. Black pixels and clear gaps are enough at menu-bar
/// size; a full-color tile of that mark is too small to read there. macOS
/// tints a template image for the light and dark menu bar.
#[cfg(any(target_os = "macos", target_os = "windows", test))]
pub fn menu_bar_rgba(size: u32) -> Vec<u8> {
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    if size == 0 {
        return rgba;
    }
    let aa = 0.7 / size as f32;
    // The mark occupies the middle of the app icon. Spread that same shape
    // across the status slot so the clip and the rules stay visible.
    let span = 0.66 / 0.86;
    let show_detail = size >= 32;
    let line_h = 0.035_f32.max(2.2 / size as f32);
    for y in 0..size {
        let py = (y as f32 + 0.5) / size as f32;
        let ny = 0.51 + (py - 0.5) * span;
        for x in 0..size {
            let px = (x as f32 + 0.5) / size as f32;
            let nx = 0.50 + (px - 0.5) * span;
            let paper = cover(round_box(nx, ny, 0.22, 0.30, 0.78, 0.84, 0.07), aa);
            let mut line = 0.0;
            if show_detail && paper > 0.5 {
                for center in [0.46_f32, 0.58, 0.70] {
                    if (0.34..=0.66).contains(&nx) && (ny - center).abs() <= line_h / 2.0 {
                        line = 1.0;
                        break;
                    }
                }
            }
            let mut clip = cover(round_box(nx, ny, 0.36, 0.18, 0.64, 0.42, 0.04), aa);
            if show_detail {
                let hole = cover(round_box(nx, ny, 0.44, 0.22, 0.56, 0.32, 0.035), aa);
                clip *= 1.0 - hole;
            }
            let coverage = (paper * (1.0 - line)).max(clip).clamp(0.0, 1.0);
            let alpha = (coverage * 255.0).round() as u8;
            let index = ((y * size + x) * 4) as usize;
            rgba[index] = 0;
            rgba[index + 1] = 0;
            rgba[index + 2] = 0;
            rgba[index + 3] = alpha;
        }
    }
    rgba
}

#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn cover(distance: f32, aa: f32) -> f32 {
    (0.5 - distance / aa).clamp(0.0, 1.0)
}

/// Signed distance to a rounded rectangle. Negative is inside, matching make-icon.py.
#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn round_box(px: f32, py: f32, left: f32, top: f32, right: f32, bottom: f32, radius: f32) -> f32 {
    let cx = (left + right) / 2.0;
    let cy = (top + bottom) / 2.0;
    let half_w = (right - left) / 2.0;
    let half_h = (bottom - top) / 2.0;
    let dx = (px - cx).abs() - half_w + radius;
    let dy = (py - cy).abs() - half_h + radius;
    let ax = dx.max(0.0);
    let ay = dy.max(0.0);
    dx.max(dy).min(0.0) + (ax * ax + ay * ay).sqrt() - radius
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

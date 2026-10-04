#[cfg(target_os = "macos")]
use objc2::Encode;

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

/// Circular Touch ID fingerprint, as a template image.
///
/// White RGB and a coverage alpha, so the sign-in button can tint it for light
/// and dark. On Mac this asks for the system Touch ID image and keeps it only
/// when that image is the fingerprint in a circle. SF Symbol `touchid` is the
/// rounded-square sensor used on iPhone, and that shape is not used here.
#[cfg(any(target_os = "macos", target_os = "windows", test))]
pub fn touch_id_rgba(size: u32) -> Vec<u8> {
    #[cfg(target_os = "macos")]
    if let Some(system) = system_circular_touch_id(size) {
        return system;
    }
    drawn_touch_id_rgba(size)
}

/// The drawn circular fingerprint. Tests use this so a rounded-square system
/// symbol cannot change the result.
#[cfg(any(target_os = "macos", target_os = "windows", test))]
pub fn drawn_touch_id_rgba(size: u32) -> Vec<u8> {
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    if size < 8 {
        return rgba;
    }
    let aa = 1.2 / size as f32;
    for y in 0..size {
        for x in 0..size {
            let nx = (x as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            let ny = (y as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            let coverage = touch_id_coverage(nx, ny, aa).clamp(0.0, 1.0);
            let index = ((y * size + x) * 4) as usize;
            rgba[index] = 255;
            rgba[index + 1] = 255;
            rgba[index + 2] = 255;
            rgba[index + 3] = (coverage * 255.0).round() as u8;
        }
    }
    rgba
}

/// True when the opaque pixels are a circle, not a square or rounded square.
#[cfg_attr(not(test), allow(dead_code))]
#[cfg(any(target_os = "macos", target_os = "windows", test))]
pub fn touch_id_glyph_is_circular(rgba: &[u8], size: u32) -> bool {
    if size < 8 || rgba.len() < (size * size * 4) as usize {
        return false;
    }
    let alpha = |x: u32, y: u32| rgba[((y * size + x) * 4 + 3) as usize];
    if alpha(1, 1) > 20 || alpha(size - 2, 1) > 20 || alpha(1, size - 2) > 20 || alpha(size - 2, size - 2) > 20 {
        return false;
    }
    let center = (size as f32 - 1.0) / 2.0;
    let mut cardinal = 0.0_f32;
    let mut opaque = 0u32;
    for y in 0..size {
        for x in 0..size {
            if alpha(x, y) < 110 {
                continue;
            }
            opaque += 1;
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let angle = dy.atan2(dx).abs();
            let axis = angle.min((std::f32::consts::FRAC_PI_2 - angle).abs());
            if axis < 0.18 {
                cardinal = cardinal.max((dx * dx + dy * dy).sqrt());
            }
        }
    }
    if opaque < size || cardinal < size as f32 * 0.25 {
        return false;
    }
    // A circle's diagonal is the same radius as its sides. A square or a
    // rounded square still has ink out along the diagonal past that radius.
    let probe = cardinal * 1.12;
    for (sx, sy) in [(1.0_f32, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
        let x = center + probe * std::f32::consts::FRAC_1_SQRT_2 * sx;
        let y = center + probe * std::f32::consts::FRAC_1_SQRT_2 * sy;
        let ix = x.round() as i32;
        let iy = y.round() as i32;
        if ix < 0 || iy < 0 || ix >= size as i32 || iy >= size as i32 {
            continue;
        }
        if alpha(ix as u32, iy as u32) > 80 {
            return false;
        }
    }
    true
}

#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn touch_id_coverage(nx: f32, ny: f32, aa: f32) -> f32 {
    let ring = stroke((nx * nx + ny * ny).sqrt() - 0.86, 0.07, aa);
    ring.max(fingerprint_coverage(nx, ny, aa))
}

/// Nested ridges, open at the bottom, the way the Touch ID fingerprint sits in its circle.
#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn fingerprint_coverage(nx: f32, ny: f32, aa: f32) -> f32 {
    let ridges = [
        // rx, ry, cx, cy, gap, phase, width
        (0.12_f32, 0.10, 0.0, -0.02, 0.28, 0.55, 0.075),
        (0.23, 0.20, 0.0, -0.03, 0.42, 0.35, 0.070),
        (0.34, 0.30, 0.0, -0.02, 0.50, 0.18, 0.066),
        (0.45, 0.40, 0.0, -0.01, 0.58, 0.08, 0.062),
        (0.56, 0.50, 0.0, 0.0, 0.70, 0.0, 0.058),
    ];
    let mut coverage = 0.0_f32;
    for (rx, ry, cx, cy, gap, phase, width) in ridges {
        let distance = distance_to_open_ellipse(nx, ny, cx, cy, rx, ry, gap, phase) - width * 0.5;
        coverage = coverage.max(cover(distance, aa));
    }
    coverage
}

/// Distance to an elliptical arc that leaves a gap at the bottom.
/// `phase` rotates that gap so the inner ridges curl into a whorl.
#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn distance_to_open_ellipse(nx: f32, ny: f32, cx: f32, cy: f32, rx: f32, ry: f32, gap: f32, phase: f32) -> f32 {
    let bottom = -std::f32::consts::FRAC_PI_2 + phase;
    let start = bottom + gap;
    let sweep = std::f32::consts::TAU - gap * 2.0;
    let steps = 28;
    let mut best = f32::MAX;
    let mut previous = None;
    for step in 0..=steps {
        let angle = start + sweep * (step as f32 / steps as f32);
        let point = (cx + rx * angle.cos(), cy - ry * angle.sin());
        if let Some(from) = previous {
            best = best.min(distance_to_segment(nx, ny, from, point));
        }
        previous = Some(point);
    }
    best
}

#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn distance_to_segment(px: f32, py: f32, from: (f32, f32), to: (f32, f32)) -> f32 {
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    let len2 = dx * dx + dy * dy;
    let t = if len2 <= f32::EPSILON {
        0.0
    } else {
        ((px - from.0) * dx + (py - from.1) * dy) / len2
    };
    let t = t.clamp(0.0, 1.0);
    let qx = from.0 + dx * t - px;
    let qy = from.1 + dy * t - py;
    (qx * qx + qy * qy).sqrt()
}

#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn stroke(signed_radius: f32, width: f32, aa: f32) -> f32 {
    cover(signed_radius.abs() - width * 0.5, aa)
}

/// macOS `touchid` is often the rounded-square sensor. Use the system image
/// only when the pixels are actually the circular fingerprint.
#[cfg(target_os = "macos")]
fn system_circular_touch_id(size: u32) -> Option<Vec<u8>> {
    let rgba = unsafe { raster_system_symbol("touchid", size) }?;
    if rgba.len() == (size * size * 4) as usize && touch_id_glyph_is_circular(&rgba, size) {
        Some(rgba)
    } else {
        None
    }
}

#[cfg(target_os = "macos")]
unsafe fn raster_system_symbol(name: &str, size: u32) -> Option<Vec<u8>> {
    use objc2::runtime::{AnyObject, Bool};
    use objc2::{class, msg_send};

    let symbol: *mut AnyObject = msg_send![class!(NSImage), imageWithSystemSymbolName: ns_string(name) accessibilityDescription: std::ptr::null::<AnyObject>()];
    if symbol.is_null() {
        return None;
    }
    let _: () = msg_send![symbol, setTemplate: Bool::YES];
    let _: () = msg_send![symbol, setSize: NSSize { width: size as f64, height: size as f64 }];
    let tiff: *mut AnyObject = msg_send![symbol, TIFFRepresentation];
    if tiff.is_null() {
        return None;
    }
    let rep: *mut AnyObject = msg_send![class!(NSBitmapImageRep), imageRepWithData: tiff];
    if rep.is_null() {
        return None;
    }
    let wide: isize = msg_send![rep, pixelsWide];
    let high: isize = msg_send![rep, pixelsHigh];
    let samples: isize = msg_send![rep, samplesPerPixel];
    if wide != size as isize || high != size as isize || samples < 3 {
        return None;
    }
    let bytes: *mut u8 = msg_send![rep, bitmapData];
    if bytes.is_null() {
        return None;
    }
    let bpp: isize = msg_send![rep, bitsPerPixel];
    let channels = (bpp / 8) as usize;
    if channels < 3 {
        return None;
    }
    let raw = std::slice::from_raw_parts(bytes, (size as usize) * (size as usize) * channels);
    Some(template_from_bitmap(raw, size, channels))
}

#[cfg(target_os = "macos")]
fn template_from_bitmap(raw: &[u8], size: u32, channels: usize) -> Vec<u8> {
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let corners_opaque = raw.len() >= channels && (channels < 4 || raw[3] > 250);
    for (index, src) in raw.chunks_exact(channels).enumerate() {
        if index >= (size * size) as usize {
            break;
        }
        let src_alpha = if channels >= 4 { src[3] } else { 255 };
        let alpha = if corners_opaque {
            let luma = (src[0] as u16 + src[1] as u16 + src[2] as u16) / 3;
            ((255 - luma) * src_alpha as u16 / 255) as u8
        } else {
            src_alpha
        };
        let dest = index * 4;
        rgba[dest] = 255;
        rgba[dest + 1] = 255;
        rgba[dest + 2] = 255;
        rgba[dest + 3] = alpha;
    }
    rgba
}

#[cfg(target_os = "macos")]
unsafe fn ns_string(value: &str) -> *mut objc2::runtime::AnyObject {
    use objc2::{class, msg_send};
    let c = std::ffi::CString::new(value).unwrap_or_else(|_| std::ffi::CString::new("").unwrap());
    msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()]
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct NSSize {
    width: f64,
    height: f64,
}

#[cfg(target_os = "macos")]
unsafe impl objc2::Encode for NSSize {
    const ENCODING: objc2::Encoding = objc2::Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING]);
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

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

/// Notification-area glyph. The clipboard mask, painted white so it reads on a
/// dark taskbar. This is not the green D in the title bar.
#[cfg(any(target_os = "windows", test))]
pub fn notification_rgba(size: u32) -> Vec<u8> {
    let mut rgba = menu_bar_rgba(size);
    for pixel in rgba.chunks_mut(4) {
        if pixel[3] == 0 {
            continue;
        }
        pixel[0] = 255;
        pixel[1] = 255;
        pixel[2] = 255;
    }
    rgba
}

/// Taskbar tile. Included here and from `build.rs`, which embeds the same
/// pixels as the exe icon. The running app loads that resource; these pixels
/// stay in the crate for the tests. Separate from the notification-area glyph.
#[cfg(any(target_os = "windows", test))]
#[allow(dead_code)]
#[path = "taskbar_icon.rs"]
mod taskbar_icon;

#[cfg(any(target_os = "windows", test))]
pub use taskbar_icon::ICON_RESOURCE_ID;

#[cfg(test)]
pub use taskbar_icon::{taskbar_rgba, TILE_RADIUS};

#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn cover(distance: f32, aa: f32) -> f32 {
    (0.5 - distance / aa).clamp(0.0, 1.0)
}

/// The row delete mark: an outline trash glyph inside a circle.
///
/// Same arrangement as the iPhone row. White RGB with coverage in alpha, so
/// the row tints it with the danger color. One tap on that control deletes.
#[cfg(any(target_os = "macos", target_os = "windows", test))]
pub fn trash_mark_rgba(size: u32) -> Vec<u8> {
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    if size == 0 {
        return rgba;
    }
    let aa = 0.8 / size as f32;
    // Strokes are sized for a 28px control drawn from a 64px mask.
    let circle_half = 1.7 / size as f32;
    let stroke_half = 1.45 / size as f32;
    // Lucide's trash-2, the simple trash glyph, scaled to sit inside the circle
    // the way the iPhone SF Symbol sits in its circle.
    let map = |x: f32, y: f32| -> (f32, f32) { (0.22 + x / 24.0 * 0.56, 0.20 + y / 24.0 * 0.60) };
    let lid = [map(3.0, 6.0), map(21.0, 6.0)];
    let handle = [map(8.0, 6.0), map(8.0, 4.2), map(10.0, 2.2), map(14.0, 2.2), map(16.0, 4.2), map(16.0, 6.0)];
    let body = [map(5.0, 6.0), map(5.0, 19.0), map(7.0, 21.0), map(17.0, 21.0), map(19.0, 19.0), map(19.0, 6.0)];
    let left_slot = [map(10.0, 11.0), map(10.0, 17.0)];
    let right_slot = [map(14.0, 11.0), map(14.0, 17.0)];
    for y in 0..size {
        let py = (y as f32 + 0.5) / size as f32;
        for x in 0..size {
            let px = (x as f32 + 0.5) / size as f32;
            let dx = px - 0.5;
            let dy = py - 0.5;
            let ring = ((dx * dx + dy * dy).sqrt() - 0.40).abs();
            let mut mark = cover(ring - circle_half, aa);
            mark = mark.max(stroke_coverage(px, py, &lid, stroke_half, aa));
            mark = mark.max(stroke_coverage(px, py, &handle, stroke_half, aa));
            mark = mark.max(stroke_coverage(px, py, &body, stroke_half, aa));
            mark = mark.max(stroke_coverage(px, py, &left_slot, stroke_half, aa));
            mark = mark.max(stroke_coverage(px, py, &right_slot, stroke_half, aa));
            let alpha = (mark.clamp(0.0, 1.0) * 255.0).round() as u8;
            let index = ((y * size + x) * 4) as usize;
            rgba[index] = 255;
            rgba[index + 1] = 255;
            rgba[index + 2] = 255;
            rgba[index + 3] = alpha;
        }
    }
    rgba
}

#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn stroke_coverage(px: f32, py: f32, points: &[(f32, f32)], half: f32, aa: f32) -> f32 {
    if points.len() < 2 {
        return 0.0;
    }
    let mut nearest = f32::MAX;
    for pair in points.windows(2) {
        nearest = nearest.min(segment_distance(px, py, pair[0].0, pair[0].1, pair[1].0, pair[1].1));
    }
    cover(nearest - half, aa)
}

#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn segment_distance(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let abx = bx - ax;
    let aby = by - ay;
    let len2 = abx * abx + aby * aby;
    let t = if len2 <= f32::EPSILON {
        0.0
    } else {
        (((px - ax) * abx + (py - ay) * aby) / len2).clamp(0.0, 1.0)
    };
    let dx = px - (ax + abx * t);
    let dy = py - (ay + aby * t);
    (dx * dx + dy * dy).sqrt()
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

/// Pixels of the official macOS SF Symbol `touchid`, as a template.
///
/// White RGB and the symbol's coverage in alpha, so the sign-in control can
/// tint it for light and dark. This is the rounded-square sensor. Nothing is
/// drawn in its place.
#[cfg(target_os = "macos")]
pub struct TouchIdSymbol {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

#[cfg(target_os = "macos")]
pub fn touch_id_symbol() -> Option<TouchIdSymbol> {
    unsafe { raster_touch_id_symbol() }
}

#[cfg(target_os = "macos")]
unsafe fn raster_touch_id_symbol() -> Option<TouchIdSymbol> {
    use objc2::runtime::{AnyObject, Bool};
    use objc2::{class, msg_send};

    let symbol: *mut AnyObject = msg_send![class!(NSImage), imageWithSystemSymbolName: ns_string("touchid") accessibilityDescription: ns_string("Touch ID")];
    if symbol.is_null() {
        return None;
    }
    let config: *mut AnyObject = msg_send![class!(NSImageSymbolConfiguration), configurationWithPointSize: 28.0_f64 weight: 0.0_f64];
    let symbol: *mut AnyObject = if config.is_null() {
        symbol
    } else {
        let configured: *mut AnyObject = msg_send![symbol, imageWithSymbolConfiguration: config];
        if configured.is_null() { symbol } else { configured }
    };
    // Template so the glyph is one color. egui tints that mask for light and dark.
    let _: () = msg_send![symbol, setTemplate: Bool::YES];

    let pixels: isize = 64;
    let rep: *mut AnyObject = msg_send![class!(NSBitmapImageRep), alloc];
    let rep: *mut AnyObject = msg_send![
        rep,
        initWithBitmapDataPlanes: std::ptr::null_mut::<*mut u8>()
        pixelsWide: pixels
        pixelsHigh: pixels
        bitsPerSample: 8_isize
        samplesPerPixel: 4_isize
        hasAlpha: Bool::YES
        isPlanar: Bool::NO
        colorSpaceName: ns_string("NSDeviceRGBColorSpace")
        bytesPerRow: 0_isize
        bitsPerPixel: 32_isize
    ];
    if rep.is_null() {
        return None;
    }
    let context: *mut AnyObject = msg_send![class!(NSGraphicsContext), graphicsContextWithBitmapImageRep: rep];
    if context.is_null() {
        return None;
    }
    let rect = NSRect {
        origin: NSPoint { x: 0.0, y: 0.0 },
        size: NSSize { width: pixels as f64, height: pixels as f64 },
    };
    let _: () = msg_send![class!(NSGraphicsContext), saveGraphicsState];
    let _: () = msg_send![class!(NSGraphicsContext), setCurrentContext: context];
    // NSImageInterpolationHigh. The bitmap starts empty; clear keeps the corners transparent.
    let _: () = msg_send![context, setImageInterpolation: 3_isize];
    NSRectFillUsingOperation(rect, 0);
    let black: *mut AnyObject = msg_send![class!(NSColor), blackColor];
    let _: () = msg_send![black, set];
    let _: () = msg_send![symbol, drawInRect: rect];
    let _: () = msg_send![class!(NSGraphicsContext), restoreGraphicsState];

    let wide: isize = msg_send![rep, pixelsWide];
    let high: isize = msg_send![rep, pixelsHigh];
    let row_bytes: isize = msg_send![rep, bytesPerRow];
    let samples: isize = msg_send![rep, samplesPerPixel];
    if wide <= 0 || high <= 0 || row_bytes < wide * 4 || samples < 4 {
        return None;
    }
    let bytes: *mut u8 = msg_send![rep, bitmapData];
    if bytes.is_null() {
        return None;
    }
    let width = wide as usize;
    let height = high as usize;
    let raw = std::slice::from_raw_parts(bytes, row_bytes as usize * height);
    Some(template_from_bitmap(raw, width, height, row_bytes as usize))
}

/// White RGB, alpha from the symbol. A cleared bitmap uses alpha. An opaque
/// white page uses darkness, so either raster still tints as one color.
#[cfg(target_os = "macos")]
fn template_from_bitmap(raw: &[u8], width: usize, height: usize, row_bytes: usize) -> TouchIdSymbol {
    let mut rgba = vec![0u8; width * height * 4];
    let corner_alpha = raw.get(3).copied().unwrap_or(0);
    let opaque_page = corner_alpha > 250;
    for y in 0..height {
        let row = &raw[y * row_bytes..];
        for x in 0..width {
            let px = &row[x * 4..x * 4 + 4];
            let alpha = if opaque_page {
                let luma = (px[0] as u16 + px[1] as u16 + px[2] as u16) / 3;
                ((255 - luma) * px[3] as u16 / 255) as u8
            } else {
                px[3]
            };
            let dest = (y * width + x) * 4;
            rgba[dest] = 255;
            rgba[dest + 1] = 255;
            rgba[dest + 2] = 255;
            rgba[dest + 3] = alpha;
        }
    }
    TouchIdSymbol { width, height, rgba }
}

#[cfg(target_os = "macos")]
unsafe fn ns_string(value: &str) -> *mut objc2::runtime::AnyObject {
    use objc2::{class, msg_send};
    let c = std::ffi::CString::new(value).unwrap_or_else(|_| std::ffi::CString::new("").unwrap());
    msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()]
}

#[cfg(target_os = "macos")]
extern "C" {
    fn NSRectFillUsingOperation(rect: NSRect, operation: usize);
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct NSPoint {
    x: f64,
    y: f64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct NSSize {
    width: f64,
    height: f64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct NSRect {
    origin: NSPoint,
    size: NSSize,
}

#[cfg(target_os = "macos")]
unsafe impl objc2::Encode for NSPoint {
    const ENCODING: objc2::Encoding = objc2::Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
}

#[cfg(target_os = "macos")]
unsafe impl objc2::Encode for NSSize {
    const ENCODING: objc2::Encoding = objc2::Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING]);
}

#[cfg(target_os = "macos")]
unsafe impl objc2::Encode for NSRect {
    const ENCODING: objc2::Encoding = objc2::Encoding::Struct("CGRect", &[NSPoint::ENCODING, NSSize::ENCODING]);
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

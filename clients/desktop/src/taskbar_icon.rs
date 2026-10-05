//! Windows taskbar tile.
//!
//! `icon.rs` and `build.rs` both include this file. The exe's embedded icon
//! and the in-process drawing are this function, so the corner radius cannot
//! drift between them.

/// Corner radius as a fraction of the tile. Just a slight round: the pixels
/// outside the arc stay clear, and the edge is anti-aliased.
pub const TILE_RADIUS: f32 = 0.045;

/// Integer id of the ICON resource `build.rs` links into the exe.
pub const ICON_RESOURCE_ID: u16 = 1;

/// Clipboard on the light plate. Corners outside the rounded tile are clear.
/// This is the taskbar button, not the notification-area glyph and not the
/// green D in the title bar.
pub fn taskbar_rgba(size: u32) -> Vec<u8> {
    const PLATE: [u8; 3] = [0xE6, 0xE2, 0xDA];
    const PAGE: [u8; 3] = [0xFF, 0xFD, 0xF8];
    const MARK: [u8; 3] = [0x1D, 0x68, 0x43];
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    if size == 0 {
        return rgba;
    }
    let aa = 0.7 / size as f32;
    let line_h = 0.035_f32.max(2.2 / size as f32);
    let show_detail = size >= 32;
    let tile = TILE_RADIUS;
    for y in 0..size {
        let ny = (y as f32 + 0.5) / size as f32;
        for x in 0..size {
            let nx = (x as f32 + 0.5) / size as f32;
            let plate = cover(round_box(nx, ny, 0.0, 0.0, 1.0, 1.0, tile), aa);
            if plate <= 0.0 {
                continue;
            }
            let mut color = [PLATE[0], PLATE[1], PLATE[2], 255];
            let paper = cover(round_box(nx, ny, 0.22, 0.30, 0.78, 0.84, 0.07), aa);
            color = blend(color, PAGE, paper);
            if show_detail && paper > 0.5 {
                for center in [0.46_f32, 0.58, 0.70] {
                    if (0.34..=0.66).contains(&nx) && (ny - center).abs() <= line_h / 2.0 {
                        color = blend(color, MARK, 0.95);
                        break;
                    }
                }
            }
            let mut clip = cover(round_box(nx, ny, 0.36, 0.18, 0.64, 0.42, 0.04), aa);
            if show_detail {
                let hole = cover(round_box(nx, ny, 0.44, 0.22, 0.56, 0.32, 0.035), aa);
                clip *= 1.0 - hole;
            }
            color = blend(color, MARK, clip);
            color[3] = (plate * 255.0).round() as u8;
            let index = ((y * size + x) * 4) as usize;
            rgba[index..index + 4].copy_from_slice(&color);
        }
    }
    rgba
}

fn blend(dst: [u8; 4], src: [u8; 3], amount: f32) -> [u8; 4] {
    if amount <= 0.0 {
        return dst;
    }
    if amount >= 1.0 {
        return [src[0], src[1], src[2], 255];
    }
    let inv = 1.0 - amount;
    [
        (dst[0] as f32 * inv + src[0] as f32 * amount).round() as u8,
        (dst[1] as f32 * inv + src[1] as f32 * amount).round() as u8,
        (dst[2] as f32 * inv + src[2] as f32 * amount).round() as u8,
        255,
    ]
}

fn cover(distance: f32, aa: f32) -> f32 {
    (0.5 - distance / aa).clamp(0.0, 1.0)
}

/// Signed distance to a rounded rectangle. Negative is inside.
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

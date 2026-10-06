//! In-memory previews of a decrypted item.
//!
//! The thumbnail and the snippet are built from the bytes already in memory.
//! Nothing here writes those bytes to a file.

use std::io::Cursor;

use image::{ImageReader, Limits};

const SAMPLE_BYTES: usize = 8_192;
const SNIPPET_CHARS: usize = 160;
const THUMB_MAX_PIXEL: u32 = 320;
const THUMB_MAX_BYTES: usize = 40 * 1024 * 1024;

/// A short plain-text snippet. Only the start of a large body is read.
pub fn text_snippet(bytes: &[u8]) -> String {
    let end = bytes.len().min(SAMPLE_BYTES);
    let sample = String::from_utf8_lossy(&bytes[..end]);
    let without_tags = strip_tags(&sample);
    let mut snippet = crate::format::text_preview(&without_tags, SNIPPET_CHARS);
    if bytes.len() > end && !snippet.is_empty() && !snippet.ends_with('…') {
        snippet.push('…');
    }
    snippet
}

/// A small JPEG for a photo. `None` when the bytes are empty, too large, or not a readable image.
pub fn thumbnail_jpeg(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.is_empty() || bytes.len() > THUMB_MAX_BYTES {
        return None;
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(16_000);
    limits.max_image_height = Some(16_000);
    limits.max_alloc = Some(128 * 1024 * 1024);
    let mut reader = ImageReader::new(Cursor::new(bytes));
    reader.limits(limits);
    let image = reader.with_guessed_format().ok()?.decode().ok()?;
    let thumb = image.thumbnail(THUMB_MAX_PIXEL, THUMB_MAX_PIXEL).into_rgb8();
    let mut jpeg = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(Cursor::new(&mut jpeg), 72);
    encoder.encode_image(&thumb).ok()?;
    if jpeg.is_empty() {
        None
    } else {
        Some(jpeg)
    }
}

fn strip_tags(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '<' {
            let mut end = index + 1;
            let mut closed = false;
            while end < chars.len() && end - index <= 300 {
                if chars[end] == '>' {
                    closed = true;
                    end += 1;
                    break;
                }
                end += 1;
            }
            if closed {
                out.push(' ');
                index = end;
                continue;
            }
        }
        out.push(chars[index]);
        index += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{text_snippet, thumbnail_jpeg};

    #[test]
    fn snippet_strips_tags_and_stays_short() {
        let html = b"<p>Hello <b>there</b></p>";
        assert_eq!(text_snippet(html), "Hello there");
        let long = "word ".repeat(80);
        let snippet = text_snippet(long.as_bytes());
        assert!(snippet.ends_with('…'));
        assert!(snippet.chars().count() <= 160);
    }

    #[test]
    fn photo_thumbnail_is_a_small_jpeg_kept_in_memory() {
        let mut png = Vec::new();
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(48, 24, image::Rgb([12, 80, 40])))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let jpeg = thumbnail_jpeg(&png).expect("png decodes");
        assert!(jpeg.starts_with(&[0xFF, 0xD8]), "jpeg starts with the SOI marker");
        assert!(jpeg.len() < 20_000, "thumbnail stays small, got {}", jpeg.len());
        assert!(thumbnail_jpeg(b"").is_none());
        assert!(thumbnail_jpeg(b"not an image").is_none());
    }
}

#!/usr/bin/env python3
"""Draw Drop's light and dark app icons into an asset catalog.

The clipboard mark is the same shape in both. The field behind it is light
in the default icon and dark in the luminosity-dark icon, which is how macOS
picks an appearance from CFBundleIconName. package-mac.sh compiles the catalog.
"""

import json
import struct
import zlib
from pathlib import Path

GREEN = (0x1D, 0x68, 0x43)
LIGHT_PLATE = (0xE6, 0xE2, 0xDA)
LIGHT_PAGE = (0xFF, 0xFD, 0xF8)
DARK_PLATE = (0x1C, 0x1C, 0x1E)
DARK_PAGE = (0xF7, 0xF3, 0xEA)

# size name, scale, pixels. 16@2x and 32@1x share a pixel size but are separate files.
SLOTS = (
    ("16x16", "1x", 16),
    ("16x16", "2x", 32),
    ("32x32", "1x", 32),
    ("32x32", "2x", 64),
    ("128x128", "1x", 128),
    ("128x128", "2x", 256),
    ("256x256", "1x", 256),
    ("256x256", "2x", 512),
    ("512x512", "1x", 512),
    ("512x512", "2x", 1024),
)


def cover(distance, aa):
    return max(0.0, min(1.0, 0.5 - distance / aa))


def round_box(px, py, left, top, right, bottom, radius):
    cx = (left + right) / 2
    cy = (top + bottom) / 2
    hw = (right - left) / 2
    hh = (bottom - top) / 2
    dx = abs(px - cx) - hw + radius
    dy = abs(py - cy) - hh + radius
    ax = max(dx, 0.0)
    ay = max(dy, 0.0)
    return min(max(dx, dy), 0.0) + (ax * ax + ay * ay) ** 0.5 - radius


def over(dst, src, amount):
    if amount <= 0:
        return dst
    if amount >= 1:
        return (src[0], src[1], src[2], 255)
    inv = 1 - amount
    return tuple(int(dst[i] * inv + src[i] * amount) for i in range(3)) + (255,)


def draw(size, plate, page):
    """RGBA bytes. Plate is the icon field. Page, clip, and rules are the mark."""
    aa = 0.7 / size
    pixels = bytearray(size * size * 4)
    line_h = max(0.035, 2.2 / size)
    show_lines = size >= 32
    show_hole = size >= 32
    for y in range(size):
        ny = (y + 0.5) / size
        for x in range(size):
            nx = (x + 0.5) / size
            color = (plate[0], plate[1], plate[2], 255)
            paper = cover(round_box(nx, ny, 0.22, 0.30, 0.78, 0.84, 0.07), aa)
            color = over(color, page, paper)
            if show_lines and paper > 0.5:
                for center in (0.46, 0.58, 0.70):
                    if 0.34 <= nx <= 0.66 and abs(ny - center) <= line_h / 2:
                        color = over(color, GREEN, 0.95)
                        break
            clip = cover(round_box(nx, ny, 0.36, 0.18, 0.64, 0.42, 0.04), aa)
            if show_hole:
                hole = cover(round_box(nx, ny, 0.44, 0.22, 0.56, 0.32, 0.035), aa)
                clip *= 1 - hole
            color = over(color, GREEN, clip)
            i = (y * size + x) * 4
            pixels[i : i + 4] = bytes(color)
    return bytes(pixels)


def png(size, rgba):
    def chunk(tag, data):
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    raw = b"".join(b"\x00" + rgba[y * size * 4 : (y + 1) * size * 4] for y in range(size))
    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")


def check(size, rgba, dark):
    def pixel(x, y):
        i = (y * size + x) * 4
        return rgba[i], rgba[i + 1], rgba[i + 2]

    assert len(rgba) == size * size * 4
    corner = pixel(0, 0)
    page = pixel(size // 2, min(size - 1, int(size * 0.78)))
    # Beside the clip hole, which shows the plate through the middle.
    clip = pixel(max(0, int(size * 0.39)), max(0, int(size * 0.28)))
    if dark:
        assert corner[0] < 50 and corner[1] < 50 and corner[2] < 55, (size, corner)
    else:
        assert corner[0] > 200 and corner[1] > 200 and corner[2] > 190, (size, corner)
    assert page[0] > 220 and page[1] > 210, (size, page)
    assert clip[1] > clip[0] + 20 and clip[1] > 70, (size, clip)


def main():
    here = Path(__file__).resolve().parent
    iconset = here / "Assets.xcassets" / "AppIcon.appiconset"
    iconset.mkdir(parents=True, exist_ok=True)
    images = []
    cache = {}
    for size_name, scale, pixels in SLOTS:
        for dark, suffix in ((False, ""), (True, "-dark")):
            key = (pixels, dark)
            rgba = cache.get(key)
            if rgba is None:
                plate = DARK_PLATE if dark else LIGHT_PLATE
                page = DARK_PAGE if dark else LIGHT_PAGE
                rgba = draw(pixels, plate, page)
                check(pixels, rgba, dark)
                cache[key] = rgba
            if scale == "1x":
                stem = f"icon_{size_name}{suffix}"
            else:
                stem = f"icon_{size_name}@{scale}{suffix}"
            (iconset / f"{stem}.png").write_bytes(png(pixels, rgba))
            entry = {
                "filename": f"{stem}.png",
                "idiom": "mac",
                "scale": scale,
                "size": size_name,
            }
            if dark:
                entry["appearances"] = [{"appearance": "luminosity", "value": "dark"}]
            images.append(entry)
    contents = {"images": images, "info": {"author": "xcode", "version": 1}}
    (iconset / "Contents.json").write_text(json.dumps(contents, indent=2) + "\n")
    catalog = iconset.parent / "Contents.json"
    catalog.write_text(json.dumps({"info": {"author": "xcode", "version": 1}}, indent=2) + "\n")
    print(f"wrote {iconset} ({len(images)} images)")


if __name__ == "__main__":
    main()

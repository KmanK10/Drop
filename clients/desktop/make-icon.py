#!/usr/bin/env python3
"""Draw Drop's app icon and write desktop/Drop.icns.

A cream clipboard on a green field. The same shapes are drawn at every size
so the mark still reads at 16px. macOS masks the square to a squircle.
"""

import struct
import zlib
from pathlib import Path

GREEN = (0x1D, 0x68, 0x43)
GREEN_DEEP = (0x14, 0x52, 0x33)
CREAM = (0xF7, 0xF3, 0xEA)
CLIP = (0x0E, 0x3D, 0x26)

# OSType, pixel size. Retina sizes repeat a dimension under another type.
ICNS_SIZES = (
    (b"icp4", 16),
    (b"icp5", 32),
    (b"icp6", 64),
    (b"ic07", 128),
    (b"ic08", 256),
    (b"ic09", 512),
    (b"ic10", 1024),
    (b"ic11", 32),
    (b"ic12", 64),
    (b"ic13", 256),
    (b"ic14", 512),
)


def mix(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))


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


def draw(size):
    """RGBA bytes, row-major, opaque."""
    aa = 0.7 / size
    pixels = bytearray(size * size * 4)
    # Lines need about two pixels or they vanish. Skip them on the smallest sizes.
    line_h = max(0.035, 2.2 / size)
    show_lines = size >= 32
    show_hole = size >= 32
    for y in range(size):
        ny = (y + 0.5) / size
        background = mix(GREEN, GREEN_DEEP, ny)
        for x in range(size):
            nx = (x + 0.5) / size
            color = (background[0], background[1], background[2], 255)
            paper = cover(round_box(nx, ny, 0.22, 0.30, 0.78, 0.84, 0.07), aa)
            color = over(color, CREAM, paper)
            if show_lines and paper > 0.5:
                for center in (0.46, 0.58, 0.70):
                    if 0.34 <= nx <= 0.66 and abs(ny - center) <= line_h / 2:
                        color = over(color, GREEN, 0.92)
                        break
            clip = cover(round_box(nx, ny, 0.36, 0.18, 0.64, 0.42, 0.04), aa)
            if show_hole:
                hole = cover(round_box(nx, ny, 0.44, 0.22, 0.56, 0.32, 0.035), aa)
                clip *= 1 - hole
            color = over(color, CLIP, clip)
            i = (y * size + x) * 4
            pixels[i : i + 4] = bytes(color)
    return bytes(pixels)


def png(size, rgba):
    def chunk(tag, data):
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    raw = b"".join(b"\x00" + rgba[y * size * 4 : (y + 1) * size * 4] for y in range(size))
    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")


def icns(parts):
    body = b"".join(tag + struct.pack(">I", 8 + len(data)) + data for tag, data in parts)
    return b"icns" + struct.pack(">I", 8 + len(body)) + body


def main():
    here = Path(__file__).resolve().parent
    parts = []
    seen = {}
    for tag, size in ICNS_SIZES:
        rgba = seen.get(size)
        if rgba is None:
            rgba = draw(size)
            seen[size] = rgba
            check(size, rgba)
        parts.append((tag, png(size, rgba)))
    out = here / "Drop.icns"
    blob = icns(parts)
    out.write_bytes(blob)
    print(f"wrote {out} ({len(blob)} bytes)")


def check(size, rgba):
    def pixel(x, y):
        i = (y * size + x) * 4
        return rgba[i], rgba[i + 1], rgba[i + 2]

    assert len(rgba) == size * size * 4, (size, len(rgba))
    corner = pixel(0, 0)
    # Between the page rules, so a line does not fail the cream check.
    paper = pixel(size // 2, int(size * 0.52))
    # Corner is the green field. The middle of the page is cream.
    assert corner[1] > corner[0] and corner[1] > 60, (size, corner)
    assert paper[0] > 200 and paper[1] > 200 and paper[2] > 180, (size, paper)


if __name__ == "__main__":
    main()

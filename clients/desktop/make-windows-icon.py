#!/usr/bin/env python3
"""Build Drop.ico for the Windows taskbar and the exe.

The clipboard is the same mark as the Mac app icon, on the light plate.
The tile is a rounded square with transparent corners, so the taskbar
button matches the other rounded icons instead of a sharp square. The
notification-area glyph is a separate white clipboard and is not this file.
"""

import importlib.util
import struct
import sys
import zlib
from pathlib import Path

SIZES = (16, 32, 64, 128, 256)
RADIUS = 0.30


def load_drawer():
    path = Path(__file__).resolve().with_name("make-icon.py")
    spec = importlib.util.spec_from_file_location("drop_make_icon", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def png(size, rgba):
    def chunk(tag, data):
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    raw = b"".join(b"\x00" + rgba[y * size * 4 : (y + 1) * size * 4] for y in range(size))
    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")


def rounded(drawer, size):
    rgba = bytearray(drawer.draw(size, drawer.LIGHT_PLATE, drawer.LIGHT_PAGE))
    aa = 0.7 / size
    for y in range(size):
        ny = (y + 0.5) / size
        for x in range(size):
            nx = (x + 0.5) / size
            plate = drawer.cover(drawer.round_box(nx, ny, 0.0, 0.0, 1.0, 1.0, RADIUS), aa)
            index = (y * size + x) * 4
            if plate <= 0:
                rgba[index : index + 4] = b"\x00\x00\x00\x00"
            else:
                rgba[index + 3] = max(0, min(255, int(round(plate * 255))))
    return bytes(rgba)


def check(size, rgba):
    def pixel(x, y):
        index = (y * size + x) * 4
        return rgba[index : index + 4]

    assert pixel(0, 0)[3] == 0, (size, "top left")
    assert pixel(size - 1, 0)[3] == 0, (size, "top right")
    assert pixel(0, size - 1)[3] == 0, (size, "bottom left")
    assert pixel(size - 1, size - 1)[3] == 0, (size, "bottom right")
    edge = pixel(size // 2, 0)
    assert edge[3] > 200, (size, "top edge", edge)
    center = pixel(size // 2, size // 2)
    assert center[3] == 255, (size, "center", center)


def pack(images):
    header = struct.pack("<HHH", 0, 1, len(images))
    entries = b""
    payload = b""
    offset = 6 + 16 * len(images)
    for width, data in images:
        stored = 0 if width >= 256 else width
        entries += struct.pack("<BBBBHHII", stored, stored, 0, 0, 1, 32, len(data), offset)
        payload += data
        offset += len(data)
    return header + entries + payload


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: make-windows-icon.py Drop.ico")
    drawer = load_drawer()
    images = []
    for size in SIZES:
        rgba = rounded(drawer, size)
        check(size, rgba)
        images.append((size, png(size, rgba)))
    out = Path(sys.argv[1])
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_bytes(pack(images))


if __name__ == "__main__":
    main()

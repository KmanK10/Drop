#!/usr/bin/env python3
"""Pack the light Drop app icons into a Windows .ico.

The pictures are the same PNGs the Mac asset catalog uses. Windows stores
one icon on the file, so this is the light plate. The window itself still
follows the system appearance.
"""

import struct
import sys
from pathlib import Path

SIZES = (
    (16, "icon_16x16.png"),
    (32, "icon_32x32.png"),
    (64, "icon_32x32@2x.png"),
    (128, "icon_128x128.png"),
    (256, "icon_256x256.png"),
)


def png_size(data: bytes) -> tuple[int, int]:
    if data[:8] != b"\x89PNG\r\n\x1a\n" or data[12:16] != b"IHDR":
        raise SystemExit("An app icon PNG is not a PNG.")
    width, height = struct.unpack(">II", data[16:24])
    return width, height


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: make-windows-icon.py Drop.ico")
    here = Path(__file__).resolve().parent
    iconset = here / "Assets.xcassets" / "AppIcon.appiconset"
    images: list[tuple[int, bytes]] = []
    for expected, name in SIZES:
        data = (iconset / name).read_bytes()
        width, height = png_size(data)
        if width != expected or height != expected:
            raise SystemExit(f"{name} is {width}x{height}, expected {expected}")
        images.append((width, data))

    out = Path(sys.argv[1])
    header = struct.pack("<HHH", 0, 1, len(images))
    entries = b""
    payload = b""
    offset = 6 + 16 * len(images)
    for width, data in images:
        stored = 0 if width >= 256 else width
        entries += struct.pack("<BBBBHHII", stored, stored, 0, 0, 1, 32, len(data), offset)
        payload += data
        offset += len(data)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_bytes(header + entries + payload)


if __name__ == "__main__":
    main()

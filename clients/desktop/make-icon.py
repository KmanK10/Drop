#!/usr/bin/env python3
"""Draw Drop's light and dark app icons.

The clipboard mark is the same shape in both. The field behind it is light
in the default icon and dark in the dark icon.

Two documents carry that pair, because current actool does not keep a
`luminosity: dark` slot on a mac app icon:

- Assets.xcassets still lists every size with a luminosity-dark appearance,
  for the bitmap sizes actool does compile.
- AppIcon.icon is the Icon Composer document actool turns into Aqua and
  DarkAqua renditions. Its background fill changes with appearance. There is
  no plain `fill` key, which would silently discard the dark fill.

package-mac.sh compiles both. The bundle names the icon with CFBundleIconName
and does not set CFBundleIconFile.
"""

import json
import struct
import sys
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


def luminance(rgb):
    return 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]


def draw_mark(size, page):
    """Clipboard only, transparent everywhere else, so the icon fill is the plate."""
    aa = 0.7 / size
    pixels = bytearray(size * size * 4)
    line_h = max(0.035, 2.2 / size)
    show_lines = size >= 32
    show_hole = size >= 32

    def composite(dst, rgb, coverage):
        if coverage <= 0:
            return dst
        src_a = 1.0 if coverage >= 1 else coverage
        dst_a = dst[3] / 255
        out_a = src_a + dst_a * (1 - src_a)
        if out_a <= 0:
            return (0, 0, 0, 0)
        out = tuple(int((rgb[i] * src_a + dst[i] * dst_a * (1 - src_a)) / out_a) for i in range(3))
        return out + (max(0, min(255, int(round(out_a * 255)))),)

    for y in range(size):
        ny = (y + 0.5) / size
        for x in range(size):
            nx = (x + 0.5) / size
            color = (0, 0, 0, 0)
            paper = cover(round_box(nx, ny, 0.22, 0.30, 0.78, 0.84, 0.07), aa)
            color = composite(color, page, paper)
            if show_lines and paper > 0.5:
                for center in (0.46, 0.58, 0.70):
                    if 0.34 <= nx <= 0.66 and abs(ny - center) <= line_h / 2:
                        color = composite(color, GREEN, 0.95)
                        break
            clip = cover(round_box(nx, ny, 0.36, 0.18, 0.64, 0.42, 0.04), aa)
            if show_hole:
                hole = cover(round_box(nx, ny, 0.44, 0.22, 0.56, 0.32, 0.035), aa)
                clip *= 1 - hole
            color = composite(color, GREEN, clip)
            i = (y * size + x) * 4
            pixels[i : i + 4] = bytes(color)
    return bytes(pixels)


def color_string(rgb):
    return "extended-srgb:{:.5f},{:.5f},{:.5f},1.00000".format(rgb[0] / 255, rgb[1] / 255, rgb[2] / 255)


def icon_document():
    """Icon Composer document. A plain fill would discard fill-specializations."""
    return {
        "fill-specializations": [
            {"value": {"solid": color_string(LIGHT_PLATE)}},
            {"appearance": "dark", "value": {"solid": color_string(DARK_PLATE)}},
        ],
        "groups": [
            {
                "name": "Clipboard",
                "layers": [
                    {
                        "name": "Mark",
                        "image-name": "mark.png",
                        "glass": False,
                    }
                ],
                "shadow": {"kind": "none", "opacity": 0},
                "specular": False,
                "translucency": {"enabled": False, "value": 0},
            }
        ],
        "supported-platforms": {"squares": ["macOS"]},
    }


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
        assert luminance(corner) < 40, (size, corner, luminance(corner))
    else:
        assert luminance(corner) > 200, (size, corner, luminance(corner))
    assert page[0] > 220 and page[1] > 210, (size, page)
    assert clip[1] > clip[0] + 20 and clip[1] > 70, (size, clip)


def check_mark(size, rgba, page):
    def pixel(x, y):
        i = (y * size + x) * 4
        return tuple(rgba[i : i + 4])

    corner = pixel(0, 0)
    paper = pixel(size // 2, min(size - 1, int(size * 0.78)))
    clip = pixel(max(0, int(size * 0.39)), max(0, int(size * 0.28)))
    assert corner[3] == 0, corner
    assert paper[3] == 255 and paper[0] > 220 and paper[1] > 210, paper
    assert clip[3] == 255 and clip[1] > clip[0] + 20 and clip[1] > 70, clip
    assert abs(paper[0] - page[0]) <= 2 and abs(paper[1] - page[1]) <= 2, paper


def read_png(path):
    data = Path(path).read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise SystemExit(f"{path} is not a PNG")
    pos = 8
    size = None
    idat = b""
    while pos + 8 <= len(data):
        length = struct.unpack(">I", data[pos : pos + 4])[0]
        tag = data[pos + 4 : pos + 8]
        chunk = data[pos + 8 : pos + 8 + length]
        pos += 12 + length
        if tag == b"IHDR":
            size, height = struct.unpack(">II", chunk[:8])
            if size != height or chunk[8] != 8 or chunk[9] != 6:
                raise SystemExit(f"{path} is not a square 8-bit RGBA PNG")
        elif tag == b"IDAT":
            idat += chunk
        elif tag == b"IEND":
            break
    raw = zlib.decompress(idat)
    stride = size * 4
    rows = []
    index = 0
    for _ in range(size):
        if raw[index] != 0:
            raise SystemExit(f"{path} uses a PNG filter this check does not decode")
        index += 1
        rows.append(raw[index : index + stride])
        index += stride
    return size, b"".join(rows)


def check_sources(here):
    iconset = here / "Assets.xcassets" / "AppIcon.appiconset"
    contents = json.loads((iconset / "Contents.json").read_text())
    images = contents["images"]
    if len(images) != len(SLOTS) * 2:
        raise SystemExit(f"expected {len(SLOTS) * 2} icon slots, found {len(images)}")
    dark_count = 0
    light_count = 0
    seen = set()
    for entry in images:
        appearances = entry.get("appearances", [])
        dark = any(
            item.get("appearance") == "luminosity" and item.get("value") == "dark" for item in appearances
        )
        if appearances and not dark:
            raise SystemExit(f"unexpected appearance on {entry.get('filename')}")
        if dark:
            dark_count += 1
        else:
            light_count += 1
        filename = entry["filename"]
        if entry["idiom"] != "mac":
            raise SystemExit(f"{filename} is not a mac icon")
        seen.add((entry["size"], entry["scale"], dark))
        size, rgba = read_png(iconset / filename)
        expected = next(pixels for size_name, scale, pixels in SLOTS if size_name == entry["size"] and scale == entry["scale"])
        if size != expected:
            raise SystemExit(f"{filename} is {size}px, expected {expected}")
        check(size, rgba, dark)
    if dark_count != len(SLOTS) or light_count != len(SLOTS):
        raise SystemExit(f"expected {len(SLOTS)} light and dark slots, found {light_count} and {dark_count}")
    for size_name, scale, _pixels in SLOTS:
        if (size_name, scale, False) not in seen or (size_name, scale, True) not in seen:
            raise SystemExit(f"missing light/dark pair for {size_name} {scale}")

    document_path = here / "AppIcon.icon" / "icon.json"
    document = json.loads(document_path.read_text())
    if "fill" in document:
        raise SystemExit("AppIcon.icon sets fill, which discards the dark fill-specializations")
    fills = document.get("fill-specializations") or []
    if not any("appearance" not in item for item in fills):
        raise SystemExit("AppIcon.icon has no light fill")
    if not any(item.get("appearance") == "dark" for item in fills):
        raise SystemExit("AppIcon.icon has no dark fill")
    light_fill = color_string(LIGHT_PLATE)
    dark_fill = color_string(DARK_PLATE)
    if fills[0]["value"]["solid"] != light_fill or fills[1]["value"]["solid"] != dark_fill:
        raise SystemExit("AppIcon.icon fill colors do not match the catalog plates")
    names = []
    for group in document["groups"]:
        for layer in group["layers"]:
            if layer.get("image-name"):
                names.append(layer["image-name"])
    if names != ["mark.png"]:
        raise SystemExit(f"AppIcon.icon layers should be the transparent mark, found {names}")
    mark_size, mark = read_png(here / "AppIcon.icon" / "Assets" / "mark.png")
    if mark_size != 1024:
        raise SystemExit(f"mark.png is {mark_size}px, expected 1024")
    check_mark(mark_size, mark, LIGHT_PAGE)

    plist = (here / "Info.plist").read_text()
    if "<key>CFBundleIconName</key>" not in plist or "<string>AppIcon</string>" not in plist:
        raise SystemExit("Info.plist must set CFBundleIconName to AppIcon")
    if "CFBundleIconFile" in plist:
        raise SystemExit("Info.plist must not set CFBundleIconFile; that selects an icns with no dark variant")
    script = (here / "package-mac.sh").read_text()
    if "desktop/Assets.xcassets" not in script or "desktop/AppIcon.icon" not in script:
        raise SystemExit("package-mac.sh must compile the asset catalog and AppIcon.icon")
    if "--app-icon AppIcon" not in script:
        raise SystemExit("package-mac.sh must pass --app-icon AppIcon")
    if "plutil -insert CFBundleIconFile" in script or "plutil -replace CFBundleIconFile" in script:
        raise SystemExit("package-mac.sh must not install CFBundleIconFile")
    print(
        f"icon sources ok: {light_count} light, {dark_count} dark, "
        f"light plate luminance {luminance(LIGHT_PLATE):.1f}, dark plate luminance {luminance(DARK_PLATE):.1f}"
    )


def main():
    here = Path(__file__).resolve().parent
    if "--check" in sys.argv:
        check_sources(here)
        return
    iconset = here / "Assets.xcassets" / "AppIcon.appiconset"
    iconset.mkdir(parents=True, exist_ok=True)
    images = []
    written = set()
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
            filename = f"{stem}.png"
            (iconset / filename).write_bytes(png(pixels, rgba))
            written.add(filename)
            entry = {
                "filename": filename,
                "idiom": "mac",
                "scale": scale,
                "size": size_name,
            }
            if dark:
                entry["appearances"] = [{"appearance": "luminosity", "value": "dark"}]
            images.append(entry)
    for stale in iconset.glob("*.png"):
        if stale.name not in written:
            stale.unlink()
    contents = {"images": images, "info": {"author": "xcode", "version": 1}}
    (iconset / "Contents.json").write_text(json.dumps(contents, indent=2) + "\n")
    catalog = iconset.parent / "Contents.json"
    catalog.write_text(json.dumps({"info": {"author": "xcode", "version": 1}}, indent=2) + "\n")

    icon_root = here / "AppIcon.icon"
    assets = icon_root / "Assets"
    assets.mkdir(parents=True, exist_ok=True)
    mark = draw_mark(1024, LIGHT_PAGE)
    check_mark(1024, mark, LIGHT_PAGE)
    (assets / "mark.png").write_bytes(png(1024, mark))
    for stale in assets.glob("*"):
        if stale.name != "mark.png":
            stale.unlink()
    (icon_root / "icon.json").write_text(json.dumps(icon_document(), indent=2) + "\n")
    check_sources(here)
    print(f"wrote {iconset} ({len(images)} images) and {icon_root}")


if __name__ == "__main__":
    main()

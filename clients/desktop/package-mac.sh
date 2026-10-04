#!/bin/sh
# Build a Mac disk image. Open the image and drag Drop into Applications.
# Run this on a Mac. It does not codesign the app.
set -eu
cd "$(dirname "$0")/.."
cargo build --release -p drop-desktop

stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT

app="$stage/Drop.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/drop "$app/Contents/MacOS/Drop"
chmod +x "$app/Contents/MacOS/Drop"
cp desktop/Info.plist "$app/Contents/Info.plist"
cp desktop/Drop.icns "$app/Contents/Resources/Drop.icns"
# Eight-byte bundle signature so Finder treats this as an application.
printf 'APPL????' > "$app/Contents/PkgInfo"

# Build the image by writing onto a mounted volume. Creating the Applications
# symlink there keeps hdiutil from copying the real Applications folder.
scratch="$stage/rw.dmg"
mount="$stage/mnt"
mkdir -p "$mount"
hdiutil create -size 256m -fs HFS+ -volname "Drop" -ov "$scratch" >/dev/null
hdiutil attach -mountpoint "$mount" -readwrite -noverify -noautoopen "$scratch" >/dev/null
cleanup_mount() {
  hdiutil detach "$mount" >/dev/null 2>&1 || hdiutil detach -force "$mount" >/dev/null 2>&1 || true
  rm -rf "$stage"
}
trap cleanup_mount EXIT

cp -R "$app" "$mount/Drop.app"
ln -s /Applications "$mount/Applications"
sync
hdiutil detach "$mount" >/dev/null
trap 'rm -rf "$stage"' EXIT

out="target/release/Drop.dmg"
rm -f "$out"
# hdiutil adds the .dmg suffix itself.
hdiutil convert "$scratch" -format UDZO -ov -o "${out%.dmg}" >/dev/null

echo "Built $out"
echo "Open it with: open \"$out\""
echo "Drag Drop onto Applications, then open Drop from the Applications folder."
echo "It stays in the menu bar. Opening the bare executable under target/ launches Terminal; use the disk image instead."

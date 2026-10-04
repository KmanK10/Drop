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
# Light and dark icons live in the asset catalog. actool writes Assets.car,
# which Launchpad, the Dock, and Finder select from CFBundleIconName.
if ! command -v xcrun >/dev/null 2>&1; then
  echo "xcrun actool is required to compile the light and dark app icon. Install Xcode or the command line tools." >&2
  exit 1
fi
xcrun actool \
  desktop/Assets.xcassets \
  --compile "$app/Contents/Resources" \
  --platform macosx \
  --minimum-deployment-target 12.0 \
  --app-icon AppIcon \
  --output-partial-info-plist "$stage/Assets.plist"
test -f "$app/Contents/Resources/Assets.car"
# Keep any icon filename actool recorded, so it matches the file it just wrote.
if [ -f "$stage/Assets.plist" ]; then
  iconfile=$(plutil -extract CFBundleIconFile raw "$stage/Assets.plist" 2>/dev/null || true)
  if [ -n "$iconfile" ]; then
    plutil -replace CFBundleIconFile -string "$iconfile" "$app/Contents/Info.plist" 2>/dev/null \
      || plutil -insert CFBundleIconFile -string "$iconfile" "$app/Contents/Info.plist"
  fi
fi
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

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
# Light and dark icons: the asset catalog lists both luminances, and
# AppIcon.icon is what current actool compiles into Aqua and DarkAqua.
# CFBundleIconName selects that car. An icns cannot switch appearance, and
# writing CFBundleIconFile makes Launchpad keep the light tile.
if ! command -v xcrun >/dev/null 2>&1; then
  echo "xcrun actool is required to compile the light and dark app icon. Install Xcode or the command line tools." >&2
  exit 1
fi
if command -v python3 >/dev/null 2>&1; then
  python3 desktop/make-icon.py --check
fi
if ! xcrun --find assetutil >/dev/null 2>&1 && ! xcrun --sdk macosx --find assetutil >/dev/null 2>&1; then
  echo "xcrun assetutil is required to confirm the dark icon was compiled. Install Xcode or the command line tools." >&2
  exit 1
fi
log="$stage/actool.txt"
car="$app/Contents/Resources/Assets.car"
car_info="$stage/car.json"
run_actool() {
  set +e
  xcrun actool "$@" >"$log" 2>&1
  status=$?
  set -e
  cat "$log" >&2
  return "$status"
}
car_has_dark() {
  rm -f "$car_info"
  if ! xcrun assetutil --info "$car" >"$car_info" 2>"$stage/assetutil.err" \
    && ! xcrun --sdk macosx assetutil --info "$car" >"$car_info" 2>>"$stage/assetutil.err"; then
    echo "assetutil could not read Assets.car." >&2
    cat "$stage/assetutil.err" >&2
    return 1
  fi
  if grep -E 'DarkAqua|UIAppearanceDark|"[Aa]ppearance" *: *"dark"|"value" *: *"dark"' "$car_info" >/dev/null; then
    return 0
  fi
  return 1
}
compile_dark_icon() {
  rm -f "$car"
  if ! run_actool "$@"; then
    echo "actool did not accept that icon compile." >&2
    return 1
  fi
  if [ ! -f "$car" ]; then
    echo "actool did not write Assets.car." >&2
    return 1
  fi
  if car_has_dark; then
    return 0
  fi
  echo "That actool run did not embed a dark appearance." >&2
  if [ -f "$car_info" ]; then
    grep -E '"Appearance"|"value"' "$car_info" | sort | uniq >&2 || true
  fi
  return 1
}
# Prefer the catalog and the icon document together. If this actool drops one
# of them, try the document alone, then the catalog alone. Stop at the first
# car that actually contains a dark appearance.
if ! compile_dark_icon \
  desktop/Assets.xcassets \
  desktop/AppIcon.icon \
  --compile "$app/Contents/Resources" \
  --platform macosx \
  --target-device mac \
  --minimum-deployment-target 12.0 \
  --app-icon AppIcon \
  --include-all-app-icons \
  --standalone-icon-behavior none \
  --enable-icon-stack-fallback-generation=disabled \
  --output-partial-info-plist "$stage/Assets.plist" \
  --warnings --errors --notices \
  && ! compile_dark_icon \
  desktop/AppIcon.icon \
  --compile "$app/Contents/Resources" \
  --platform macosx \
  --target-device mac \
  --minimum-deployment-target 12.0 \
  --app-icon AppIcon \
  --include-all-app-icons \
  --standalone-icon-behavior none \
  --output-partial-info-plist "$stage/Assets.plist" \
  --warnings --errors --notices \
  && ! compile_dark_icon \
  desktop/Assets.xcassets \
  --compile "$app/Contents/Resources" \
  --platform macosx \
  --target-device mac \
  --minimum-deployment-target 12.0 \
  --app-icon AppIcon \
  --output-partial-info-plist "$stage/Assets.plist" \
  --warnings --errors --notices
then
  echo "actool compiled no dark app icon. Dark mode would keep showing the light tile." >&2
  exit 1
fi
test -f "$car"
# A loose icns is the light picture only. Remove it so it cannot override the car.
rm -f "$app/Contents/Resources/"*.icns
iconname=$(plutil -extract CFBundleIconName raw "$app/Contents/Info.plist")
if [ "$iconname" != "AppIcon" ]; then
  echo "CFBundleIconName must be AppIcon so macOS selects the compiled icon." >&2
  exit 1
fi
# Do not merge CFBundleIconFile out of actool's partial plist.
plutil -remove CFBundleIconFile "$app/Contents/Info.plist" 2>/dev/null || true
if plutil -extract CFBundleIconFile raw "$app/Contents/Info.plist" >/dev/null 2>&1; then
  echo "CFBundleIconFile is still set. Launchpad would show the light icon in dark mode." >&2
  exit 1
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

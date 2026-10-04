#!/bin/sh
# Wrap the release binary in a menu-bar app bundle. Run this on a Mac.
set -eu
cd "$(dirname "$0")/.."
cargo build --release -p drop-desktop
app="target/release/Drop.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS"
cp target/release/drop "$app/Contents/MacOS/Drop"
cp desktop/Info.plist "$app/Contents/Info.plist"
echo "Built $app"
echo "Open it with: open $app"

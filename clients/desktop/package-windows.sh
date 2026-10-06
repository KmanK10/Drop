#!/bin/sh
# Build a Windows installer. Copy DropSetup.exe to Windows and run it.
# The installer is unsigned. It does not start Drop at login.
set -eu
cd "$(dirname "$0")/.."

if ! command -v makensis >/dev/null 2>&1; then
  echo "makensis is required. On Debian or Ubuntu: sudo apt install nsis" >&2
  exit 1
fi
if grep -E 'CurrentVersion\\Run|RunOnce' desktop/drop.nsi >/dev/null; then
  echo "The installer must not start Drop at login." >&2
  exit 1
fi

cargo build --release -p drop-desktop --target x86_64-pc-windows-gnu

exe="target/x86_64-pc-windows-gnu/release/drop.exe"
icon="target/x86_64-pc-windows-gnu/release/Drop.ico"
out="target/x86_64-pc-windows-gnu/release/DropSetup.exe"
if [ ! -f "$exe" ]; then
  echo "The Windows build did not produce $exe" >&2
  exit 1
fi
if [ ! -f "$icon" ]; then
  echo "The Windows build did not produce $icon" >&2
  exit 1
fi

makensis -NOCD -INPUTCHARSET UTF8 \
  -DSOURCE_EXE="$(pwd)/$exe" \
  -DICON="$(pwd)/$icon" \
  -DOUTFILE="$(pwd)/$out" \
  desktop/drop.nsi

echo "Wrote $out"
echo "Copy DropSetup.exe to Windows and run it. It installs Drop for the current user, adds a Start menu shortcut, and does not codesign."

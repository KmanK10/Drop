# Drop native clients

Windows, Mac, and iPhone clients for an existing Drop server. They use the same Argon2id split and AES-256-GCM layout as the browser. The server still stores ciphertext only.

After you sign in and enter the password once, the session cookie and the content key stay in the process until you quit the app. Closing the desktop window leaves Drop in the notification area or menu bar. Quitting, or signing out, drops both. The password and the content key are not written to disk, Keychain, or UserDefaults. The only file that is saved is the server address and the username.

There is no public signup. An admin still creates the account in the browser.

The server field starts empty. Type the address of your own Drop server. The placeholder `https://drop.example` is not a real host. `http://` is accepted for a local server and the window says the connection is not HTTPS. A server address you already saved is left as it is.

## What stays the same

- Argon2id, 19 MiB, 2 iterations, parallelism 1, 16-byte salt, 64-byte output, version 0x13. The password is NFKC-normalized first.
- The first 32 bytes are SHA-256 hashed into the auth verifier. The last 32 bytes are the AES-256-GCM content key and are never sent.
- Ciphertext is a 12-byte IV, then ciphertext, then the 16-byte tag.
- Item bytes use the `DRP1` layout from `src/shared/item.ts`.
- Mutations send `x-drop-request: 1` and do not send an `Origin` header, which is how a non-browser client passes the existing CSRF check.
- A hostile server cannot ask the client to run a weaker KDF, or a KDF so large it is a denial of service. Parameters outside memory 19456–262144 KiB, time 2–16, and parallelism 1–4 are refused. The real server parameters sit inside that range.

`clients/core` is checked against the browser's Argon2 test vector, a known AES-GCM vector, and a live server round trip (`cargo test -p drop-core` from this directory, with `npm ci` already run at the repo root).

## Desktop

The desktop app is Rust (egui). One window works on Windows and Mac. The content key lives on a worker thread, not in the UI, and not in the config file.

On Windows it adds a notification-area icon. On Mac it adds a menu-bar icon and hides the Dock icon. Drag a file onto that icon to upload it to the signed-in account. Click the icon to open the window, where you can copy or download items, paste text, or pick a file. On Mac, right-click the icon for Open, Sign out, and Quit. On Windows, right-click the icon for the same menu.

Explorer paints the Windows icon, so a file drop is caught by a small layered window that appears over the icon only while a drag is already in progress. A click that starts on the icon still opens Drop.

The window's close button hides Drop. Quit is the control that ends the process and forgets the key. On Windows 11 the icon can land in the notification-area overflow; drag it onto the visible row if you want it beside the clock.

The window follows the system appearance, the same way the website follows `prefers-color-scheme`. Light mode stays the cream palette. Dark mode uses the website's dark colors. The Mac menu-bar icon is a template image, so the menu bar already adapts.

Config paths, server address and username only:

- Windows: `%APPDATA%\Drop\config.json`
- Mac: `~/Library/Application Support/Drop/config.json`
- Linux is not a supported desktop target. `cargo run -p drop-desktop` there prints a short message and exits.

### Windows

From `clients/`, with a recent stable Rust and the GNU Windows target:

```bash
rustup target add x86_64-pc-windows-gnu
cargo build --release -p drop-desktop --target x86_64-pc-windows-gnu
```

The program is `target/x86_64-pc-windows-gnu/release/drop.exe`. Run that exe on Windows. This repository was cross-compiled with `x86_64-w64-mingw32-gcc` (see `clients/.cargo/config.toml`). A real notification-area drag still has to be tried on Windows; this environment cannot host the Windows shell.

### Mac

Build this on a Mac (the menu-bar code links AppKit). The script makes a disk image, not an App Store package, and it does not codesign:

```bash
cd clients
./desktop/package-mac.sh
open target/release/Drop.dmg
```

`open` mounts the image. Drag Drop onto the Applications folder in that window, then open Drop from Applications. It is a menu-bar app (`LSUIElement`), so it does not stay in the Dock. The window follows the Mac appearance. The app icon does too: `desktop/Assets.xcassets` has a light clipboard and a dark clipboard, and `package-mac.sh` compiles that catalog with `actool` into the bundle (`CFBundleIconName` is `AppIcon`). Launchpad, the Dock, and Finder then use the light field in light mode and the dark field in dark mode. Rebuild the pictures with `python3 desktop/make-icon.py` only if you change the drawing. `actool` comes with Xcode or its command line tools.

Do not double-click the executable inside `target/`. Finder runs that bare file in Terminal, and the path is several folders down. `cargo run --release -p drop-desktop` also works while you are developing; the process sets the accessory activation policy itself.

This Linux environment cannot compile or run the Mac tray, and it cannot build the disk image (`hdiutil` is a Mac tool). The AppKit drop target and the image still need a Mac.

## iPhone

The Xcode project is `clients/ios/Drop.xcodeproj`. It is an iPhone app plus a share extension. iOS 17 or later.

The app keeps the content key in memory and the session cookie in an ephemeral `URLSession`. Neither goes into the Keychain. The password field does not use a username or password content type, so iOS is not asked to store the password.

Add items from the share sheet or with Paste. The share extension only copies the file into the app-group inbox (`group.com.kiefermenard.drop`) and opens `dropclipboard://inbox`. It does not have the content key and does not upload. The running app encrypts the file and then deletes the inbox copy. If Drop was not signed in, the file waits in the inbox until you sign in.

iOS cannot drop a file on the Dynamic Island. This app does not try. A Live Activity is not included.

Open the project on a Mac, in Xcode:

1. Select the Drop target and the DropShare target. Set your development team on both.
2. The bundle ids are `com.kiefermenard.drop` and `com.kiefermenard.drop.share`. Change them if those ids are taken, and keep the app group `group.com.kiefermenard.drop` on both targets (or change that string in both entitlements and in `Shared/Inbox.swift` together).
3. Run the Drop scheme on a simulator or a phone. Then run DropShare once so the share sheet can see it, or install the app which embeds the extension.
4. Crypto checks that do not need a simulator:

```bash
cd clients/ios
swift test
```

`swift test` runs the Argon2 and AES-GCM vectors on the Mac. Argon2 is the vendored PHC reference in `clients/ios/Vendor/argon2` (CC0 or Apache-2.0). AES-GCM is CryptoKit, whose combined layout is the same IV || ciphertext || tag the browser uses.

This environment cannot codesign the app or start an iOS simulator. Building, signing, and the share sheet still need the Mac.

# Drop native clients

Windows, Mac, and iPhone clients for an existing Drop server. They use the same Argon2id split and AES-256-GCM layout as the browser. The server still stores ciphertext only.

After you sign in and enter the password once, the session cookie and the content key stay in the process until you quit the app. Closing the desktop window hides Drop in the notification area or menu bar. Turn that close setting off and closing the window quits. Signing out drops the session and the content key. The password is never written down. The saved files are the server address, the username, and `window.json` for the close setting.

Biometric unlock is off until you turn it on after that password sign-in. On iPhone that is Face ID or Touch ID, on Mac it is Touch ID, and on Windows it is Windows Hello. Turning it on stores the content key in the platform keychain, together with the session cookie that key needs, and a later unlock can release them without the password. The password is never stored. If biometrics fail, are canceled, or are off, type the password. Sign out deletes the keychain item. Quit leaves that item in place when biometric unlock is on, and on the desktop it also keeps the server session so the next launch can use it.

PIN unlock is optional and off until you set a PIN after a password sign-in. It is for a machine with no biometrics, and it can stay on beside Face ID, Touch ID, or Windows Hello. The PIN is 4 to 8 digits. Drop stores an Argon2id and AES-256-GCM wrap of that same unlock blob. The PIN is not stored. The Drop password is not stored. The content key is not written in a normal file. On Mac the wrap is a login-keychain item, service `com.kiefermenard.drop.mac.pin`, account `unlock`, available when the Mac is unlocked, with no Touch ID gate. On Windows it is a Credential Manager generic credential named `com.kiefermenard.drop/pin`. On iPhone it is a keychain item, service `com.kiefermenard.drop.pin`, account `unlock`, this device only, with no biometry flag and not in the app group. A wrong PIN leaves the account and the wrap in place. Sign out deletes the wrap. Quit leaves it, and on the desktop quit also keeps the server session when PIN unlock or biometric unlock is on. When both are off, quit signs out of the server. When a PIN is set, typing those digits on the sign-in screen signs in without another button. The first sign-in shows the server, the username, and the password. After that, those two are remembered in the saved settings file, and the password and the content key are not. The next sign-in shows only the PIN field when a PIN is set, and only the password field when it is not. Server and username stay off that screen. Use password sits directly above Change server or account and switches to the password field. Use PIN switches back. The PIN screen has no Sign in button and does not say “Use 4 to 8 digits.” That line is only on Set PIN and Change PIN. A placeholder that repeats the title above the field is blank. The server placeholder stays `https://drop.example`. Change server or account opens the server and username again, with the password, so a different account can sign in. Set PIN, Change PIN, and Change password live in iPhone Settings and in the Mac and Windows menus beside the PIN checkmark. Delete account is at the bottom of iPhone Settings, in a danger zone, and in those same menus. It asks for the username, then a second Delete account action, and calls `DELETE /api/account`. That route needs the current session, deletes that user and their items, and refuses the last admin. A successful delete signs out and removes the PIN wrap and the biometric unlock. A refusal leaves them in place. The fields open only after you choose one of those, and they are not left on the clipboard. Change password uses `POST /api/account/password/start`, re-encrypts each item with `PUT /api/account/password/items/:id` and `x-drop-rekey`, then `POST /api/account/password/commit`. On iPhone, Mac, and Windows, Change password does not ask for the PIN. When a PIN is set, the screen says “Changing the password turns the PIN off.” A successful password change deletes the stored PIN wrap, turns PIN unlock off, and says “Password changed. PIN is off.” The user sets the PIN again afterward. The password and the PIN are not stored.

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

On Windows it adds a notification-area icon. On Mac a menu-bar icon is always there; click it to open the window. While that window is open or minimized, Drop also has a Dock icon. Drag a file onto the Dock icon to upload it. A drag onto the menu-bar icon still uploads when macOS delivers it; Mission Control takes most drags at the top of the screen, so the Dock icon is the one to use. On Mac, the application menu named Drop, immediately to the right of the Apple menu, leaves About, Hide, and Services where macOS puts them. Under that, the order is a divider, Close to menu bar, a divider, Touch ID when the Mac has it, PIN or Set PIN, Change PIN when a PIN is set, Change password, a divider, Sign out, Delete account, a divider, and Quit Drop. Close to menu bar is on by default and shows a checkmark when it is on. While it is on, closing the window hides Drop and leaves the menu-bar icon, and the Dock icon goes away. Turn it off and closing the window quits Drop. The choice is saved in `window.json` next to the server address, not in the keychain. Touch ID and PIN show a checkmark when on, and there is no caption. Set PIN, Change PIN, Change password, and Delete account open their fields in a separate window, not on the clipboard. Delete account is a danger zone: type the username, then press Delete account again. The Dock icon menu and a right-click on the status icon start with Open, then that same order. The Mac window does not show Sign out, Quit, or a Touch ID checkbox. On Windows, right-click the notification icon for Open, then a divider, Close to notification area, a divider, Windows Hello when the PC has it, PIN or Set PIN, Change PIN when a PIN is set, Change password, a divider, Sign out, Delete account, a divider, and Quit. Windows Hello and PIN are checkmarks with no caption. Close to notification area is on by default and uses that same `window.json` key. Turn it off and closing the window quits. Left-click still opens the window. The Windows window does not show Sign out, Quit, or a Windows Hello checkbox. Drag a file onto the icon to upload it.

Explorer paints the Windows icon, so a file drop is caught by a small layered window that appears over the icon only while a drag is already in progress. A click that starts on the icon still opens Drop.

The window's close button hides Drop while Close to menu bar or Close to notification area is on. Quit, in the Mac application menu or the Windows notification-area menu, ends the process. On Windows 11 the icon can land in the notification-area overflow; drag it onto the visible row if you want it beside the clock.

Touch ID stores the unlock blob in the data protection keychain, which is the keychain that can require Touch ID. The password is not stored. Sign out still deletes the item. An unsigned local build has no keychain-access-groups entitlement, so macOS rejects that save (usually keychain error -34018). There is no public call that keeps the item biometric-gated and still succeeds on an unsigned binary. This environment cannot make the keychain call, so that failure is not something this tree has watched succeed.

The window follows the system appearance, the same way the website follows `prefers-color-scheme`. Light mode stays the cream palette. Dark mode uses the website's dark colors. The Mac menu-bar icon is the same clipboard mark as the app icon, drawn as a template glyph so it stays readable at menu-bar size and follows the menu bar's light and dark color. The Windows notification-area icon is that same clipboard silhouette, painted in Drop green, because the notification area does not tint a template image.

On Windows and Mac, each clipboard row has Copy and Download as buttons, and a red trash mark in a circle on the other side. One click copies, downloads, or deletes. There is no extra menu when Download is already a button.

Config paths, server address and username only:

- Windows: `%APPDATA%\Drop\config.json` for the server address and username. `window.json` in that same folder remembers Close to notification area.
- Mac: `~/Library/Application Support/Drop/config.json` for the server address and username. `window.json` in that same folder remembers Close to menu bar.
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

`open` mounts the image. Drag Drop onto the Applications folder in that window, then open Drop from Applications. Replace an older Drop that is already in Applications. The menu-bar icon stays for the life of the process. The Dock icon is there while the window is open or minimized, and it leaves when the window is closed back to the menu bar. Drop a file on the Dock icon to upload it. The window follows the Mac appearance.

The app icon follows it too. `desktop/Assets.xcassets` has the clipboard on a light field and, with a `luminosity: dark` appearance, on a dark field. `desktop/AppIcon.icon` is the Icon Composer document current `actool` actually compiles into separate Aqua and DarkAqua renditions: the same mark, with the light plate as the default fill and the dark plate as the dark fill. `package-mac.sh` compiles both into `Assets.car`. `CFBundleIconName` is `AppIcon`. The script does not set `CFBundleIconFile` and deletes any `.icns` `actool` writes, because an icns is the light picture only and Launchpad prefers it over the dark rendition. If Launchpad still shows the previous tile after you replace the app, refresh Launch Services:

```bash
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f /Applications/Drop.app
```

Rebuild the pictures with `python3 desktop/make-icon.py` only if you change the drawing (`python3 desktop/make-icon.py --check` reads the luminances and appearances back). `actool` comes with Xcode or its command line tools.

Do not double-click the executable inside `target/`. Finder runs that bare file in Terminal, and the path is several folders down. `cargo run --release -p drop-desktop` also works while you are developing. The process shows the Dock icon while the window is open and removes it when the window is closed.

This Linux environment cannot compile or run the Mac tray, and it cannot build the disk image (`hdiutil` is a Mac tool). The AppKit drop target and the image still need a Mac.

## iPhone

The Xcode project is `clients/ios/Drop.xcodeproj`. It is an iPhone app plus a share extension. iOS 17 or later.

The app keeps the content key in memory and the session cookie in an ephemeral `URLSession` until you turn on Face ID or Touch ID. That switch is off until a password sign-in, and it lives on the Settings screen. The password field does not use a username or password content type, so iOS is not asked to store the password. The content key is not written into the app-group inbox.

The screen follows the system appearance, with the same cream and dark colors as the website. Typed text uses those colors, so it stays readable in light and dark. A tap outside a text field resigns the keyboard. There is no dismiss button in the window.

The home-screen icon is the same clipboard as the Mac app, including the light plate and the dark plate. `desktop/make-icon.py` writes both 1024 pictures into `ios/App/Assets.xcassets/AppIcon.appiconset`. The dark picture is the `luminosity: dark` appearance. iOS masks the square.

The note box, Save text, Paste, and the circled + sit in the same scroll view as the items, so the note scrolls up with the list. A gear icon and the account line stay at the top. The gear opens Settings. There is no menu on that screen. Settings has the Face ID, Touch ID, or Optic ID switch, with no caption under it. It also has Set PIN when no PIN is set, Change PIN when one is set, and Change password. Those buttons open the fields. A PIN that is on can be turned off with the PIN switch. Sign out is on Settings, above a danger zone. Delete account is in that zone. The first tap shows a username field. The account is deleted only after the username matches and Delete account is pressed again. The first sign-in shows the server, the username, and the password. After those are saved, the next sign-in shows only the PIN field when a PIN is set, otherwise only the password field. That sign-in screen does not say “Use 4 to 8 digits.” That line is only on Set PIN and Change PIN. When a PIN is set, Use password sits directly above Change server or account and switches the screen to the password field, still without the server and username. That password screen says Use PIN in the same place and switches back. The PIN sign-in screen has no Sign in button. Placeholders that repeat the title above a field are blank. The server placeholder stays `https://drop.example`. A blank password on that screen says “Enter your password.” The first-time screen still says to enter the server, username, and password. Face ID, Touch ID, or Optic ID is an icon on the right of Sign in. Change server or account opens the server and username again. Typing a correct 4 to 8 digit PIN signs in without another button. A wrong PIN does not remove the account. Changing the password uses the account API: `POST /api/account/password/start` with the current auth verifier, `PUT /api/account/password/items/:id` for each item re-encrypted under the new content key (`x-drop-rekey`), then `POST /api/account/password/commit` with the new registration material. A failed attempt `DELETE`s that rekey and keeps the old key. Change password does not ask for the current PIN. When a PIN is set, the screen says “Changing the password turns the PIN off.” After it succeeds, the stored PIN wrap is deleted and PIN unlock is off. The line is “Password changed. PIN is off.” Set PIN is how to turn it on again. The password and both content keys stay on the phone. The + button, in a circle, opens from the button: take a photo, choose a photo, or choose a file. Each of those is encrypted and uploaded the same way as a picked file. Each clipboard row has Copy beside that circled +, which holds Share and Download. A red trash icon in a matching circle sits on the other side and deletes on one tap. A photo shows a small in-memory preview. A note or text file shows a short snippet. Other files show the name and size.

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

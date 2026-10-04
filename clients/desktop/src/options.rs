//! Items in the Windows window menu. The notification-area menu uses the same
//! list after Open. The order matches the Mac Drop menu, without Open.
//!
//! The window shows these items in one title-bar menu. The labels are too wide
//! to sit on the bar themselves, the way File and Edit do.

/// The menu name drawn on the title bar.
pub const WINDOW_MENU_LABEL: &str = "Options";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowsOption {
    CloseToNotification,
    Hello,
    Pin,
    ChangePin,
    ChangePassword,
    SignOut,
    DeleteAccount,
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowsEntry {
    Command {
        label: &'static str,
        command: WindowsOption,
        checked: bool,
    },
    Divider,
}

/// Close to notification area, then Windows Hello when the PC has it, then PIN
/// or Set PIN, Change PIN only when a PIN is set, Change password, Sign out,
/// Delete account, and Quit. Close is checked when hiding to the notification
/// area is on.
pub fn windows_menu(hello_available: bool, hello_on: bool, pin_on: bool, close_on: bool) -> Vec<WindowsEntry> {
    let mut items = vec![
        WindowsEntry::Command {
            label: "Close to notification area",
            command: WindowsOption::CloseToNotification,
            checked: close_on,
        },
        WindowsEntry::Divider,
    ];
    if hello_available {
        items.push(WindowsEntry::Command {
            label: "Windows Hello",
            command: WindowsOption::Hello,
            checked: hello_on,
        });
    }
    if pin_on {
        items.push(WindowsEntry::Command {
            label: "PIN",
            command: WindowsOption::Pin,
            checked: true,
        });
        items.push(WindowsEntry::Command {
            label: "Change PIN",
            command: WindowsOption::ChangePin,
            checked: false,
        });
    } else {
        items.push(WindowsEntry::Command {
            label: "Set PIN",
            command: WindowsOption::Pin,
            checked: false,
        });
    }
    items.push(WindowsEntry::Command {
        label: "Change password",
        command: WindowsOption::ChangePassword,
        checked: false,
    });
    items.push(WindowsEntry::Divider);
    items.push(WindowsEntry::Command {
        label: "Sign out",
        command: WindowsOption::SignOut,
        checked: false,
    });
    items.push(WindowsEntry::Command {
        label: "Delete account",
        command: WindowsOption::DeleteAccount,
        checked: false,
    });
    items.push(WindowsEntry::Divider);
    items.push(WindowsEntry::Command {
        label: "Quit",
        command: WindowsOption::Quit,
        checked: false,
    });
    items
}

/// Where the title-bar menu sits, in screen pixels.
///
/// `buttons_left` and `buttons_top` are the caption-button rectangle relative
/// to the outer window. The menu stays on that caption row, after the icon,
/// and stops before the buttons.
pub fn title_bar_menu_bounds(
    window_left: i32,
    window_top: i32,
    visible_left: i32,
    buttons_left: i32,
    buttons_top: i32,
    buttons_height: i32,
    icon_width: i32,
    menu_width: i32,
    menu_height: i32,
) -> (i32, i32, i32, i32) {
    let pad = (buttons_height - icon_width).max(0) / 2;
    let x = visible_left + pad + icon_width + 2;
    let caption_top = window_top + buttons_top;
    let y = caption_top + (buttons_height - menu_height).max(0) / 2;
    let limit = window_left + buttons_left - 4;
    let width = menu_width.min((limit - x).max(0));
    (x, y, width, menu_height.max(1))
}

/// Menu text on a title-bar caption. `background` is a COLORREF, `0x00BBGGRR`.
/// A dark caption gets light text. A light caption gets dark text.
pub fn title_bar_menu_ink(background: u32, active: bool) -> u32 {
    if bgr_luma(background) < 148 {
        if active { 0x00ff_ffff } else { 0x00b4_b4b4 }
    } else if active {
        0x001c_1c1c
    } else {
        0x0066_6666
    }
}

/// A slight wash for the open or hovered label. It stays near the caption
/// color, so the label does not become a light button.
pub fn title_bar_menu_hover(background: u32) -> u32 {
    let luma = bgr_luma(background);
    let mix = |channel: u32| -> u32 {
        if luma < 148 {
            channel + (255 - channel) * 18 / 100
        } else {
            channel * 90 / 100
        }
    };
    let red = mix(background & 0xff);
    let green = mix((background >> 8) & 0xff);
    let blue = mix((background >> 16) & 0xff);
    red | (green << 8) | (blue << 16)
}

fn bgr_luma(background: u32) -> u32 {
    let red = background & 0xff;
    let green = (background >> 8) & 0xff;
    let blue = (background >> 16) & 0xff;
    (red * 3 + green * 6 + blue) / 10
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(entries: &[WindowsEntry]) -> String {
        entries
            .iter()
            .map(|entry| match entry {
                WindowsEntry::Divider => "---".to_string(),
                WindowsEntry::Command { label, checked, .. } => {
                    if *checked {
                        format!("[x] {label}")
                    } else {
                        (*label).to_string()
                    }
                }
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }

    #[test]
    fn menu_matches_the_mac_order_without_open() {
        let with_hello_and_pin = picture(&windows_menu(true, true, true, true));
        assert_eq!(
            with_hello_and_pin,
            "[x] Close to notification area | --- | [x] Windows Hello | [x] PIN | Change PIN | Change password | --- | Sign out | Delete account | --- | Quit"
        );
        let plain = picture(&windows_menu(false, false, false, false));
        assert_eq!(
            plain,
            "Close to notification area | --- | Set PIN | Change password | --- | Sign out | Delete account | --- | Quit"
        );
        assert!(!plain.contains("Open"));
        assert!(!plain.contains("Change PIN"));
        assert!(!plain.contains("Windows Hello"));
    }

    #[test]
    fn the_title_bar_menu_is_named_options() {
        assert_eq!(WINDOW_MENU_LABEL, "Options");
    }

    #[test]
    fn title_bar_menu_sits_after_the_icon_and_before_the_buttons() {
        let (x, y, width, height) = title_bar_menu_bounds(10, 20, 18, 300, 0, 32, 16, 70, 22);
        assert_eq!((x, y, width, height), (44, 25, 70, 22));
        assert!(x > 18 + 16, "past the icon");
        assert!(x + width < 10 + 300, "stops before the caption buttons");
        let (_, _, clamped, _) = title_bar_menu_bounds(10, 20, 18, 50, 0, 32, 16, 70, 22);
        assert!(clamped < 70);
        assert!(clamped > 0);
        let (x, y, _, height) = title_bar_menu_bounds(10, 20, 18, 300, 0, 32, 16, 70, 32);
        assert_eq!((x, y, height), (44, 20, 32), "a caption-tall menu fills that row");
    }

    #[test]
    fn title_bar_menu_text_follows_the_caption() {
        let dark = 0x0020_2020;
        assert_eq!(title_bar_menu_ink(dark, true), 0x00ff_ffff);
        assert_ne!(title_bar_menu_ink(dark, false), 0x00ff_ffff);
        let light = title_bar_menu_ink(0x00ff_ffff, true);
        assert!(light & 0xff < 40 && (light >> 8) & 0xff < 40, "dark text on a light caption {light:#x}");
        let hover = title_bar_menu_hover(dark);
        assert_ne!(hover, dark);
        assert!(hover & 0xff < 0x70, "hover stays on the dark caption {hover:#x}");
        assert!((hover >> 8) & 0xff < 0x70);
        assert!((hover >> 16) & 0xff < 0x70);
    }
}

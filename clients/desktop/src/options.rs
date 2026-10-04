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
    }
}

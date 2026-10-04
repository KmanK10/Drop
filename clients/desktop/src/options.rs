//! Items in the Windows window menu. The notification-area menu uses the same
//! list after Open. The order matches the Mac Drop menu, without Open.

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
}

#![cfg_attr(not(any(target_os = "windows", target_os = "macos")), allow(dead_code))]

use std::path::PathBuf;
use std::sync::mpsc::Receiver;

/// The drop overlay covers the notification icon only when a drag enters it.
/// A press that starts on the icon is a click, so the overlay stays hidden
/// and the icon still receives it. `button_was_down` is the button state from
/// the previous mouse event, which catches a click even when the cursor was
/// not recorded as already inside.
#[cfg(any(target_os = "windows", test))]
pub fn drag_should_cover(was_inside: bool, button_was_down: bool, inside: bool, down: bool) -> bool {
    let click_started_here = inside && down && !button_was_down;
    inside && !was_inside && down && !click_started_here
}

pub enum TrayAction {
    Open,
    SignOut,
    Quit,
    /// `true` turns biometrics on, `false` deletes that keychain item.
    SetBiometric(bool),
    /// `true` opens Set PIN. `false` deletes the stored PIN wrap.
    SetPin(bool),
    ChangePin,
    ChangePassword,
    DeleteAccount,
    Dropped(Vec<PathBuf>),
}

pub struct TrayPorts {
    pub events: Receiver<TrayAction>,
    pub set_tooltip: Box<dyn Fn(String) + Send>,
    pub shutdown: Box<dyn FnOnce() + Send>,
}

#[cfg(test)]
mod tests {
    use super::drag_should_cover;

    #[test]
    fn a_click_on_the_icon_does_not_cover_it() {
        assert!(!drag_should_cover(false, false, true, true));
        assert!(!drag_should_cover(true, false, true, true));
        assert!(!drag_should_cover(true, false, true, false));
    }

    #[test]
    fn a_drag_that_enters_the_icon_covers_it() {
        assert!(drag_should_cover(false, true, true, true));
        assert!(!drag_should_cover(false, true, false, true));
        assert!(!drag_should_cover(true, true, true, true));
    }
}

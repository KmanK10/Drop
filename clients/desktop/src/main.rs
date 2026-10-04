#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod app;
#[cfg(any(target_os = "windows", target_os = "macos"))]
mod icon;
#[cfg(any(target_os = "windows", target_os = "macos"))]
mod tray;
#[cfg(target_os = "macos")]
mod mac_tray;
#[cfg(target_os = "windows")]
mod win_tray;
#[cfg(any(target_os = "windows", target_os = "macos"))]
mod worker;
#[cfg(any(target_os = "windows", target_os = "macos"))]
mod biometric;

fn main() {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        if let Err(error) = app::run() {
            eprintln!("{error}");
            #[cfg(target_os = "windows")]
            win_tray::report_error(&error);
            std::process::exit(1);
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        eprintln!("The Drop desktop app runs on Windows and macOS. See clients/README.md.");
        std::process::exit(2);
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod icon;

#[cfg(test)]
mod icon_tests {
    #[test]
    fn tray_icon_is_a_green_disc_with_a_clear_corner() {
        let rgba = super::icon::tray_rgba(32);
        assert_eq!(rgba.len(), 32 * 32 * 4);
        assert_eq!(&rgba[0..4], &[0, 0, 0, 0]);
        let center = (16 * 32 + 16) * 4;
        assert!(rgba[center] < 80);
        assert!(rgba[center + 1] > 80);
        assert!(rgba[center + 3] > 200);
    }
}

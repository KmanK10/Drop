#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod app;
#[cfg(any(target_os = "windows", target_os = "macos"))]
mod icon;
#[cfg(any(target_os = "windows", target_os = "macos"))]
mod tray;
#[cfg(any(target_os = "windows", target_os = "macos"))]
mod pin;
#[cfg(any(target_os = "windows", target_os = "macos"))]
mod window_prefs;
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

    #[test]
    fn menu_bar_icon_is_the_clipboard_mark() {
        let size = 36u32;
        let rgba = super::icon::menu_bar_rgba(size);
        assert_eq!(rgba.len(), (size * size * 4) as usize);
        let pixel = |x: u32, y: u32| {
            let index = ((y * size + x) * 4) as usize;
            (rgba[index], rgba[index + 1], rgba[index + 2], rgba[index + 3])
        };
        let outside = pixel(1, 18);
        assert_eq!(outside.3, 0, "beside the page");
        let page = pixel(17, 28);
        assert_eq!((page.0, page.1, page.2), (0, 0, 0));
        assert!(page.3 > 200, "page {page:?}");
        let rule = pixel(17, 20);
        assert!(rule.3 < 40, "a rule cut out of the page {rule:?}");
        let clip = pixel(12, 8);
        assert!(clip.3 > 200, "clip {clip:?}");
        let hole = pixel(17, 6);
        assert!(hole.3 < 40, "hole in the clip {hole:?}");
    }

    #[test]
    fn touch_id_sign_in_glyph_is_a_circle_not_a_rounded_square() {
        let size = 64u32;
        let rgba = super::icon::drawn_touch_id_rgba(size);
        assert!(super::icon::touch_id_glyph_is_circular(&rgba, size));
        let pixel = |x: u32, y: u32| rgba[((y * size + x) * 4 + 3) as usize];
        assert_eq!(pixel(1, 1), 0, "the square corner stays clear");
        let center = size / 2;
        assert!(pixel(center, 4) > 150, "the circle is drawn");
        assert!(pixel(center, center) < 40, "the middle of the whorl is open");
        let mut ridges = 0;
        let mut gaps = 0;
        for y in 20..44 {
            if pixel(center, y) > 120 {
                ridges += 1;
            } else if pixel(center, y) < 30 {
                gaps += 1;
            }
        }
        assert!(ridges > 4, "fingerprint ridges {ridges}");
        assert!(gaps > 4, "gaps between ridges {gaps}");
        let shown = super::icon::touch_id_rgba(size);
        assert!(super::icon::touch_id_glyph_is_circular(&shown, size));
    }
}

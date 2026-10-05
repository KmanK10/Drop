#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod app;
#[cfg(any(target_os = "windows", target_os = "macos"))]
mod icon;
#[cfg(any(target_os = "windows", target_os = "macos"))]
mod tray;
#[cfg(target_os = "windows")]
mod options;
#[cfg(all(test, not(target_os = "windows")))]
mod options;
#[cfg(all(test, not(any(target_os = "windows", target_os = "macos"))))]
mod tray;
#[cfg(any(target_os = "windows", target_os = "macos"))]
mod pin;
#[cfg(any(target_os = "windows", target_os = "macos"))]
mod window_prefs;
#[cfg(target_os = "macos")]
mod mac_tray;
#[cfg(target_os = "windows")]
mod win_menu;
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
    fn notification_icon_is_a_white_clipboard() {
        let size = 36u32;
        let rgba = super::icon::notification_rgba(size);
        let pixel = |x: u32, y: u32| {
            let index = ((y * size + x) * 4) as usize;
            (rgba[index], rgba[index + 1], rgba[index + 2], rgba[index + 3])
        };
        assert_eq!(pixel(1, 18).3, 0, "beside the page");
        let page = pixel(17, 28);
        assert_eq!((page.0, page.1, page.2), (255, 255, 255));
        assert!(page.3 > 200, "page {page:?}");
        let clip = pixel(12, 8);
        assert_eq!((clip.0, clip.1, clip.2), (255, 255, 255));
        assert!(clip.3 > 200, "clip {clip:?}");
    }

    #[test]
    fn taskbar_icon_corners_are_transparent() {
        let size = 64u32;
        let rgba = super::icon::taskbar_rgba(size);
        let pixel = |x: u32, y: u32| {
            let index = ((y * size + x) * 4) as usize;
            (rgba[index], rgba[index + 1], rgba[index + 2], rgba[index + 3])
        };
        assert_eq!(pixel(0, 0).3, 0, "top left");
        assert_eq!(pixel(size - 1, 0).3, 0, "top right");
        assert_eq!(pixel(0, size - 1).3, 0, "bottom left");
        assert_eq!(pixel(size - 1, size - 1).3, 0, "bottom right");
        assert_eq!(pixel(1, 1).3, 0, "the corner is still rounded {:?}", pixel(1, 1));
        assert!(pixel(4, 4).3 > 200, "a Windows 11-sized radius covers this pixel {:?}", pixel(4, 4));
        let edge = pixel(size / 2, 0);
        assert!(edge.3 > 200, "middle of the top edge stays on the tile {edge:?}");
        let page = pixel(size / 2, (size as f32 * 0.78) as u32);
        assert!(page.3 > 200 && page.0 > 220, "page {page:?}");
        let center = pixel(size / 2, size / 2);
        assert_eq!(center.3, 255, "the tile is opaque inside the rounded square");
    }

    #[test]
    fn trash_mark_is_an_outline_glyph_in_a_circle() {
        let size = 64u32;
        let rgba = super::icon::trash_mark_rgba(size);
        let pixel = |x: u32, y: u32| {
            let index = ((y * size + x) * 4) as usize;
            rgba[index + 3]
        };
        assert_eq!(pixel(0, 0), 0, "outside the circle");
        assert!(pixel(6, 32) > 180, "circle stroke");
        assert!(pixel(32, 22) > 180, "lid");
        assert!(pixel(32, 16) > 180, "handle");
        assert!(pixel(21, 32) > 120, "body side");
        assert!(pixel(32, 32) < 40, "the can is an outline, not a filled blob");
        assert!(pixel(32, 48) < 40, "open inside the circle, under the can");
    }
}

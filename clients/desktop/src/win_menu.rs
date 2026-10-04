//! The window's menu bar, drawn on the title-bar row.
//!
//! `SetMenu` on the main window would put the bar under the caption. This
//! window is an owned popup that sits on the caption itself: after the icon,
//! before minimize, maximize, and close. The icon, the caption buttons, and
//! the taskbar icon stay the system's.

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use windows::core::{w, PCSTR, PCWSTR};
use windows::Win32::Foundation::{BOOL, HINSTANCE, HWND, LPARAM, LRESULT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DwmSetWindowAttribute, DWMWA_CAPTION_BUTTON_BOUNDS, DWMWA_EXTENDED_FRAME_BOUNDS,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
};
use windows::Win32::Graphics::Gdi::{
    CreateFontIndirectW, DeleteObject, GetDC, GetTextExtentPoint32W, ReleaseDC, SelectObject, HDC,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CallWindowProcW, CreateMenu, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
    DrawMenuBar,
    DestroyWindow, EnumWindows, GetClassNameW, GetForegroundWindow, GetMenu, GetMenuItemRect, GetWindowRect,
    GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, LoadCursorW, RegisterClassW,
    RemoveMenu, SetForegroundWindow, SetMenu, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    SystemParametersInfoW,
    GWLP_WNDPROC, HMENU, IDC_ARROW, MF_BYPOSITION, MF_CHECKED, MF_POPUP, MF_SEPARATOR, MF_STRING,
    SM_CXSMICON, SM_CXSIZE, SM_CYCAPTION, SM_CYMENU, SPI_GETHIGHCONTRAST, SPI_GETNONCLIENTMETRICS, SWP_NOACTIVATE,
    SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNOACTIVATE, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, WM_COMMAND,
    WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_EXITMENULOOP, WM_INITMENUPOPUP, WM_MOVE, WM_NCACTIVATE,
    WM_SETTINGCHANGE, WM_SHOWWINDOW, WM_SIZE, WM_WINDOWPOSCHANGED, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
};

use crate::options::{WindowsOption, WINDOW_MENU_LABEL};

const ID_CLOSE: usize = 101;
const ID_HELLO: usize = 102;
const ID_PIN: usize = 103;
const ID_CHANGE_PIN: usize = 104;
const ID_PASSWORD: usize = 105;
const ID_SIGNOUT: usize = 106;
const ID_DELETE: usize = 107;
const ID_QUIT: usize = 108;

const CLASS: &str = "DropMenuBar";

static INSTALLED: AtomicBool = AtomicBool::new(false);
static INSTALLING: AtomicBool = AtomicBool::new(false);
static PLACING: AtomicBool = AtomicBool::new(false);
static OWNER: AtomicUsize = AtomicUsize::new(0);
static MENU: AtomicUsize = AtomicUsize::new(0);
static POPUP: AtomicUsize = AtomicUsize::new(0);
static PREV_PROC: AtomicUsize = AtomicUsize::new(0);
static MENU_W: AtomicI32 = AtomicI32::new(0);
static MENU_H: AtomicI32 = AtomicI32::new(0);
static COVER_W: AtomicI32 = AtomicI32::new(0);
static CACHED_DPI: AtomicU32 = AtomicU32::new(0);
static SNAPPED: AtomicBool = AtomicBool::new(false);
static LAST_X: AtomicI32 = AtomicI32::new(i32::MIN);
static LAST_Y: AtomicI32 = AtomicI32::new(i32::MIN);
static LAST_W: AtomicI32 = AtomicI32::new(0);
static LAST_H: AtomicI32 = AtomicI32::new(0);

static TX: Mutex<Option<mpsc::Sender<WindowsOption>>> = Mutex::new(None);
static RX: Mutex<Option<mpsc::Receiver<WindowsOption>>> = Mutex::new(None);
static WAKE: Mutex<Option<Arc<dyn Fn() + Send + Sync>>> = Mutex::new(None);

pub fn sync(ctx: &eframe::egui::Context) {
    if !INSTALLED.load(Ordering::Acquire) {
        let ctx = ctx.clone();
        let _ = install(Arc::new(move || ctx.request_repaint()));
    }
    place();
}

pub fn poll() -> Option<WindowsOption> {
    let guard = RX.lock().ok()?;
    guard.as_ref()?.try_recv().ok()
}

fn install(wake: Arc<dyn Fn() + Send + Sync>) -> bool {
    if INSTALLED.load(Ordering::Acquire) {
        return true;
    }
    if INSTALLING.swap(true, Ordering::AcqRel) {
        return false;
    }
    let ok = unsafe { install_inner(wake) };
    INSTALLING.store(false, Ordering::Release);
    ok
}

unsafe fn install_inner(wake: Arc<dyn Fn() + Send + Sync>) -> bool {
    let Some(owner) = find_main_window() else {
        return false;
    };
    if TX.lock().ok().and_then(|guard| guard.as_ref().map(|_| ())).is_none() {
        let (tx, rx) = mpsc::channel();
        if let Ok(mut slot) = TX.lock() {
            *slot = Some(tx);
        }
        if let Ok(mut slot) = RX.lock() {
            *slot = Some(rx);
        }
    }
    if let Ok(mut slot) = WAKE.lock() {
        *slot = Some(wake);
    }

    let module = match GetModuleHandleW(None) {
        Ok(module) => module,
        Err(_) => return false,
    };
    let instance = HINSTANCE(module.0);
    // The class keeps this pointer. It has to outlive install.
    let class_name = w!("DropMenuBar");
    let class = WNDCLASSW {
        lpfnWndProc: Some(menu_proc),
        hInstance: instance,
        lpszClassName: class_name,
        hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
        ..Default::default()
    };
    RegisterClassW(&class);

    let bar = match CreateMenu() {
        Ok(menu) => menu,
        Err(_) => return false,
    };
    let popup = match CreatePopupMenu() {
        Ok(menu) => menu,
        Err(_) => {
            let _ = DestroyMenu(bar);
            return false;
        }
    };
    let label = wide(WINDOW_MENU_LABEL);
    if AppendMenuW(bar, MF_POPUP | MF_STRING, popup.0 as usize, PCWSTR(label.as_ptr())).is_err() {
        let _ = DestroyMenu(popup);
        let _ = DestroyMenu(bar);
        return false;
    }
    fill_popup(popup);
    allow_dark_menus();

    let dpi = window_dpi(owner);
    let (width, height, cover) = measure(dpi);
    MENU_W.store(width, Ordering::Relaxed);
    MENU_H.store(height, Ordering::Relaxed);
    COVER_W.store(cover, Ordering::Relaxed);
    CACHED_DPI.store(dpi, Ordering::Relaxed);
    SNAPPED.store(false, Ordering::Relaxed);

    let menu = match CreateWindowExW(
        WS_EX_TOOLWINDOW,
        class_name,
        w!(""),
        WS_POPUP,
        0,
        0,
        width,
        height,
        owner,
        None,
        instance,
        None,
    ) {
        Ok(hwnd) => hwnd,
        Err(_) => {
            let _ = DestroyMenu(bar);
            return false;
        }
    };
    if SetMenu(menu, bar).is_err() {
        let _ = DestroyWindow(menu);
        let _ = DestroyMenu(bar);
        return false;
    }
    let _ = DrawMenuBar(menu);
    let corner = DWMWCP_DONOTROUND;
    let _ = DwmSetWindowAttribute(
        menu,
        DWMWA_WINDOW_CORNER_PREFERENCE,
        &corner as *const _ as *const _,
        std::mem::size_of_val(&corner) as u32,
    );
    allow_dark_for_window(menu);

    OWNER.store(owner.0 as usize, Ordering::Release);
    MENU.store(menu.0 as usize, Ordering::Release);
    POPUP.store(popup.0 as usize, Ordering::Release);

    let prev = SetWindowLongPtrW(owner, GWLP_WNDPROC, owner_proc as *const () as usize as isize);
    if prev == 0 {
        let _ = DestroyWindow(menu);
        MENU.store(0, Ordering::Release);
        POPUP.store(0, Ordering::Release);
        OWNER.store(0, Ordering::Release);
        return false;
    }
    PREV_PROC.store(prev as usize, Ordering::Release);
    INSTALLED.store(true, Ordering::Release);
    true
}

fn place() {
    if PLACING.swap(true, Ordering::AcqRel) {
        return;
    }
    let _guard = PlaceGuard;
    unsafe { place_inner() }
}

struct PlaceGuard;

impl Drop for PlaceGuard {
    fn drop(&mut self) {
        PLACING.store(false, Ordering::Release);
    }
}

unsafe fn place_inner() {
    let owner = hwnd_of(OWNER.load(Ordering::Acquire));
    let menu = hwnd_of(MENU.load(Ordering::Acquire));
    if owner.is_invalid() || menu.is_invalid() || !IsWindow(owner).as_bool() || !IsWindow(menu).as_bool() {
        return;
    }
    if !IsWindowVisible(owner).as_bool() || IsIconic(owner).as_bool() {
        if IsWindowVisible(menu).as_bool() {
            let _ = ShowWindow(menu, SW_HIDE);
        }
        LAST_X.store(i32::MIN, Ordering::Relaxed);
        return;
    }

    let dpi = window_dpi(owner);
    if dpi != CACHED_DPI.swap(dpi, Ordering::Relaxed) {
        let (width, height, cover) = measure(dpi);
        MENU_W.store(width, Ordering::Relaxed);
        MENU_H.store(height, Ordering::Relaxed);
        COVER_W.store(cover, Ordering::Relaxed);
        SNAPPED.store(false, Ordering::Relaxed);
    }

    move_menu(owner, menu);
    // The item width is only real once the bar has been shown.
    snap_to_item(menu);
    if !SNAPPED.load(Ordering::Relaxed) {
        return;
    }
    move_menu(owner, menu);
}

unsafe fn move_menu(owner: HWND, menu: HWND) {
    let width = MENU_W.load(Ordering::Relaxed).max(1);
    let height = MENU_H.load(Ordering::Relaxed).max(1);
    let Some((x, y, w, h)) = caption_slot(owner, width, height) else {
        return;
    };
    let visible = IsWindowVisible(menu).as_bool();
    if visible
        && LAST_X.load(Ordering::Relaxed) == x
        && LAST_Y.load(Ordering::Relaxed) == y
        && LAST_W.load(Ordering::Relaxed) == w
        && LAST_H.load(Ordering::Relaxed) == h
    {
        return;
    }
    let flags = if visible { SWP_NOACTIVATE | SWP_NOZORDER } else { SWP_NOACTIVATE | SWP_NOZORDER | SWP_SHOWWINDOW };
    if SetWindowPos(menu, None, x, y, w, h, flags).is_ok() {
        LAST_X.store(x, Ordering::Relaxed);
        LAST_Y.store(y, Ordering::Relaxed);
        LAST_W.store(w, Ordering::Relaxed);
        LAST_H.store(h, Ordering::Relaxed);
    } else {
        let _ = ShowWindow(menu, SW_SHOWNOACTIVATE);
    }
}

unsafe fn snap_to_item(menu: HWND) {
    if SNAPPED.load(Ordering::Relaxed) {
        return;
    }
    if !IsWindowVisible(menu).as_bool() {
        return;
    }
    let bar = GetMenu(menu);
    if bar.is_invalid() {
        return;
    }
    let mut rect = RECT::default();
    if GetMenuItemRect(menu, bar, 0, &mut rect).is_err() {
        return;
    }
    let item = rect.right - rect.left;
    if item <= 0 {
        return;
    }
    let width = item.max(COVER_W.load(Ordering::Relaxed)).max(1);
    MENU_W.store(width, Ordering::Relaxed);
    SNAPPED.store(true, Ordering::Relaxed);
    LAST_W.store(0, Ordering::Relaxed);
}

unsafe fn caption_slot(owner: HWND, menu_width: i32, menu_height: i32) -> Option<(i32, i32, i32, i32)> {
    let mut window = RECT::default();
    GetWindowRect(owner, &mut window).ok()?;
    let mut visible = window;
    let _ = DwmGetWindowAttribute(
        owner,
        DWMWA_EXTENDED_FRAME_BOUNDS,
        &mut visible as *mut RECT as *mut _,
        std::mem::size_of::<RECT>() as u32,
    );
    let mut buttons = RECT::default();
    let buttons_ok = DwmGetWindowAttribute(
        owner,
        DWMWA_CAPTION_BUTTON_BOUNDS,
        &mut buttons as *mut RECT as *mut _,
        std::mem::size_of::<RECT>() as u32,
    )
    .is_ok();
    let dpi = window_dpi(owner);
    let icon = GetSystemMetricsForDpi(SM_CXSMICON, dpi).max(16);
    let (buttons_left, buttons_top, buttons_height) = if buttons_ok && buttons.right > buttons.left && buttons.bottom > buttons.top {
        (buttons.left, buttons.top, buttons.bottom - buttons.top)
    } else {
        let caption = GetSystemMetricsForDpi(SM_CYCAPTION, dpi).max(menu_height);
        let button = GetSystemMetricsForDpi(SM_CXSIZE, dpi).max(16);
        let width = (window.right - window.left).max(button * 3);
        (width - button * 3, 0, caption)
    };
    let bounds = crate::options::title_bar_menu_bounds(
        window.left,
        window.top,
        visible.left,
        buttons_left,
        buttons_top,
        buttons_height,
        icon,
        menu_width,
        menu_height,
    );
    if bounds.2 <= 0 {
        return None;
    }
    Some(bounds)
}

unsafe fn measure(dpi: u32) -> (i32, i32, i32) {
    let mut bar = RECT { left: 0, top: 0, right: 10, bottom: 0 };
    let fitted = AdjustWindowRectExForDpi(&mut bar, WS_POPUP, BOOL(1), WS_EX_TOOLWINDOW, dpi)
        .ok()
        .map(|_| bar.bottom - bar.top)
        .filter(|height| *height > 0);
    let height = fitted.unwrap_or_else(|| GetSystemMetricsForDpi(SM_CYMENU, dpi).max(1));
    let scale = |px: i32| px * dpi as i32 / 96;
    let fallback_item = scale(72);
    let fallback_cover = scale(36);
    let Some((item, cover)) = text_widths(dpi) else {
        return (fallback_item, height, fallback_cover);
    };
    let width = item.max(cover).max(1);
    (width, height, cover.max(1))
}

unsafe fn text_widths(dpi: u32) -> Option<(i32, i32)> {
    let mut metrics = windows::Win32::UI::WindowsAndMessaging::NONCLIENTMETRICSW::default();
    metrics.cbSize = std::mem::size_of_val(&metrics) as u32;
    SystemParametersInfoW(
        SPI_GETNONCLIENTMETRICS,
        metrics.cbSize,
        Some(&mut metrics as *mut _ as *mut _),
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
    )
    .ok()?;
    let dc = GetDC(None);
    if dc.is_invalid() {
        return None;
    }
    let item = font_width(dc, &metrics.lfMenuFont, WINDOW_MENU_LABEL).unwrap_or(0);
    let cover = font_width(dc, &metrics.lfCaptionFont, "Drop").unwrap_or(0);
    let _ = ReleaseDC(None, dc);
    if item <= 0 {
        return None;
    }
    let pad = (16 * dpi as i32 / 96).max(12);
    Some((item + pad, cover + 4))
}

unsafe fn font_width(dc: HDC, font: &windows::Win32::Graphics::Gdi::LOGFONTW, text: &str) -> Option<i32> {
    let font = CreateFontIndirectW(font);
    if font.is_invalid() {
        return None;
    }
    let previous = SelectObject(dc, font);
    let chars = wide_chars(text);
    let mut size = SIZE::default();
    let ok = GetTextExtentPoint32W(dc, &chars, &mut size).as_bool();
    if !previous.is_invalid() {
        SelectObject(dc, previous);
    }
    let _ = DeleteObject(font);
    if !ok || size.cx <= 0 {
        None
    } else {
        Some(size.cx)
    }
}

unsafe fn fill_popup(popup: HMENU) {
    while RemoveMenu(popup, 0, MF_BYPOSITION).is_ok() {}
    for entry in crate::options::windows_menu(
        crate::biometric::available(),
        crate::biometric::enrolled(),
        crate::pin::enrolled(),
        crate::window_prefs::close_to_menu_bar(),
    ) {
        match entry {
            crate::options::WindowsEntry::Divider => {
                let _ = AppendMenuW(popup, MF_SEPARATOR, 0, PCWSTR::null());
            }
            crate::options::WindowsEntry::Command { label, command, checked } => {
                let flags = if checked { MF_STRING | MF_CHECKED } else { MF_STRING };
                let text = wide(label);
                let _ = AppendMenuW(popup, flags, command_id(command), PCWSTR(text.as_ptr()));
            }
        }
    }
}

fn command_id(command: WindowsOption) -> usize {
    match command {
        WindowsOption::CloseToNotification => ID_CLOSE,
        WindowsOption::Hello => ID_HELLO,
        WindowsOption::Pin => ID_PIN,
        WindowsOption::ChangePin => ID_CHANGE_PIN,
        WindowsOption::ChangePassword => ID_PASSWORD,
        WindowsOption::SignOut => ID_SIGNOUT,
        WindowsOption::DeleteAccount => ID_DELETE,
        WindowsOption::Quit => ID_QUIT,
    }
}

fn option_from_id(id: usize) -> Option<WindowsOption> {
    Some(match id {
        ID_CLOSE => WindowsOption::CloseToNotification,
        ID_HELLO => WindowsOption::Hello,
        ID_PIN => WindowsOption::Pin,
        ID_CHANGE_PIN => WindowsOption::ChangePin,
        ID_PASSWORD => WindowsOption::ChangePassword,
        ID_SIGNOUT => WindowsOption::SignOut,
        ID_DELETE => WindowsOption::DeleteAccount,
        ID_QUIT => WindowsOption::Quit,
        _ => return None,
    })
}

fn emit(command: WindowsOption) {
    let tx = TX.lock().ok().and_then(|guard| guard.clone());
    if let Some(tx) = tx {
        let _ = tx.send(command);
    }
    let wake = WAKE.lock().ok().and_then(|guard| guard.clone());
    if let Some(wake) = wake {
        wake();
    }
}

unsafe extern "system" fn menu_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => return LRESULT(1),
        WM_INITMENUPOPUP => {
            let opened = wparam.0 as *mut core::ffi::c_void;
            let popup = POPUP.load(Ordering::Acquire) as *mut core::ffi::c_void;
            if !popup.is_null() && opened == popup {
                fill_popup(HMENU(popup));
            }
        }
        WM_COMMAND => {
            if wparam.0 >> 16 == 0 {
                if let Some(command) = option_from_id(wparam.0 & 0xffff) {
                    emit(command);
                }
            }
            return LRESULT(0);
        }
        WM_EXITMENULOOP => {
            // The menu had the foreground. Give it back to the main window
            // when this process still has it, so a click on another app is left alone.
            let owner = hwnd_of(OWNER.load(Ordering::Acquire));
            let foreground = GetForegroundWindow();
            let mut pid = 0u32;
            let _ = GetWindowThreadProcessId(foreground, Some(&mut pid));
            if !owner.is_invalid() && pid == GetCurrentProcessId() {
                let _ = SetForegroundWindow(owner);
                let _ = SetFocus(owner);
            }
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

unsafe extern "system" fn owner_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_NCACTIVATE && wparam.0 == 0 {
        let menu = hwnd_of(MENU.load(Ordering::Acquire));
        let foreground = GetForegroundWindow();
        let activating_menu = !menu.is_invalid() && (foreground == menu || lparam.0 == menu.0 as isize);
        if activating_menu {
            return call_prev(hwnd, msg, WPARAM(1), LPARAM(-1));
        }
    }
    let result = call_prev(hwnd, msg, wparam, lparam);
    match msg {
        WM_WINDOWPOSCHANGED | WM_MOVE | WM_SIZE | WM_SHOWWINDOW | WM_DPICHANGED => place(),
        WM_SETTINGCHANGE => {
            let menu = hwnd_of(MENU.load(Ordering::Acquire));
            if !menu.is_invalid() {
                allow_dark_menus();
                allow_dark_for_window(menu);
                let _ = DrawMenuBar(menu);
            }
        }
        WM_DESTROY => {
            let menu = hwnd_of(MENU.swap(0, Ordering::AcqRel));
            POPUP.store(0, Ordering::Release);
            OWNER.store(0, Ordering::Release);
            INSTALLED.store(false, Ordering::Release);
            if !menu.is_invalid() && IsWindow(menu).as_bool() {
                let _ = DestroyWindow(menu);
            }
        }
        _ => {}
    }
    result
}

unsafe fn call_prev(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let prev = PREV_PROC.load(Ordering::Acquire);
    if prev == 0 {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let proc: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT = std::mem::transmute(prev);
    CallWindowProcW(Some(proc), hwnd, msg, wparam, lparam)
}

fn hwnd_of(value: usize) -> HWND {
    HWND(value as *mut core::ffi::c_void)
}

unsafe fn window_dpi(hwnd: HWND) -> u32 {
    let dpi = GetDpiForWindow(hwnd);
    if dpi == 0 { 96 } else { dpi }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn wide_chars(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

fn find_main_window() -> Option<HWND> {
    let mut found: Option<HWND> = None;
    unsafe {
        let _ = EnumWindows(Some(enum_main_window), LPARAM(&mut found as *mut Option<HWND> as isize));
    }
    found
}

unsafe extern "system" fn enum_main_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let slot = &mut *(lparam.0 as *mut Option<HWND>);
    let mut pid = 0u32;
    let _ = GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid != GetCurrentProcessId() {
        return BOOL(1);
    }
    let class = window_string(hwnd, true);
    if class == "DropTrayWindow" || class == "DropTrayOverlay" || class == CLASS {
        return BOOL(1);
    }
    if window_string(hwnd, false) == "Drop" {
        *slot = Some(hwnd);
        return BOOL(0);
    }
    BOOL(1)
}

fn window_string(hwnd: HWND, class_name: bool) -> String {
    let mut buf = [0u16; 64];
    let len = unsafe {
        if class_name {
            GetClassNameW(hwnd, &mut buf)
        } else {
            GetWindowTextW(hwnd, &mut buf)
        }
    };
    if len <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..len as usize])
}

fn apps_use_dark() -> bool {
    unsafe {
        let Ok(module) = LoadLibraryW(w!("uxtheme.dll")) else {
            return false;
        };
        let Some(should) = ordinal(module, 132) else {
            return false;
        };
        let should: unsafe extern "system" fn() -> bool = std::mem::transmute(should);
        if !should() || high_contrast() {
            return false;
        }
        true
    }
}

fn high_contrast() -> bool {
    #[repr(C)]
    struct HighContrast {
        size: u32,
        flags: u32,
        scheme: *mut u16,
    }
    let mut info = HighContrast {
        size: std::mem::size_of::<HighContrast>() as u32,
        flags: 0,
        scheme: std::ptr::null_mut(),
    };
    unsafe {
        if SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            info.size,
            Some(&mut info as *mut HighContrast as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .is_err()
        {
            return false;
        }
    }
    info.flags & 1 != 0
}

fn allow_dark_menus() {
    if !apps_use_dark() {
        return;
    }
    unsafe {
        let Ok(module) = LoadLibraryW(w!("uxtheme.dll")) else {
            return;
        };
        if let Some(set_mode) = ordinal(module, 135) {
            let set_mode: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(set_mode);
            set_mode(1);
        }
        if let Some(flush) = ordinal(module, 136) {
            let flush: unsafe extern "system" fn() = std::mem::transmute(flush);
            flush();
        }
    }
}

fn allow_dark_for_window(hwnd: HWND) {
    if !apps_use_dark() {
        return;
    }
    unsafe {
        let Ok(module) = LoadLibraryW(w!("uxtheme.dll")) else {
            return;
        };
        if let Some(allow) = ordinal(module, 133) {
            let allow: unsafe extern "system" fn(HWND, i32) -> i32 = std::mem::transmute(allow);
            allow(hwnd, 1);
        }
        if let Some(set_theme) = GetProcAddress(module, PCSTR(b"SetWindowTheme\0".as_ptr())) {
            let set_theme: unsafe extern "system" fn(HWND, PCWSTR, PCWSTR) -> i32 = std::mem::transmute(set_theme);
            let name = w!("DarkMode_Explorer");
            set_theme(hwnd, name, PCWSTR::null());
        }
        if let Some(flush) = ordinal(module, 136) {
            let flush: unsafe extern "system" fn() = std::mem::transmute(flush);
            flush();
        }
    }
}

unsafe fn ordinal(module: windows::Win32::Foundation::HMODULE, ordinal: usize) -> Option<unsafe extern "system" fn() -> isize> {
    GetProcAddress(module, PCSTR(ordinal as *const u8))
}

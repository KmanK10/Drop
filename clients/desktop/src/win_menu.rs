//! The window's menu, drawn on the title-bar row.
//!
//! A system menu bar stays a light rectangle on a dark caption. This window is
//! an owned popup on the caption itself, after the icon and before minimize,
//! maximize, and close. It is filled with the caption color and the word
//! Options is drawn in the caption's text color. The dropdown is the system
//! menu. The icon, the caption buttons, and the taskbar icon stay the system's.

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use windows::core::{w, PCSTR, PCWSTR};
use windows::Win32::Foundation::{BOOL, COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_BUTTON_BOUNDS,
    DWMWA_EXTENDED_FRAME_BOUNDS, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontIndirectW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect, GetDC, GetPixel,
    GetTextExtentPoint32W, InvalidateRect, ReleaseDC, SelectObject, SetBkMode, SetTextColor, DT_LEFT, DT_NOPREFIX,
    DT_SINGLELINE, DT_VCENTER, HDC, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::Input::KeyboardAndMouse::{SetFocus, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CallWindowProcW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    EnumWindows, GetClassNameW, GetClientRect, GetForegroundWindow, GetWindowRect, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, LoadCursorW, PostMessageW, RegisterClassW,
    RemoveMenu, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, SystemParametersInfoW, TrackPopupMenu,
    GWLP_WNDPROC, HMENU, IDC_ARROW, MF_BYPOSITION, MF_CHECKED, MF_SEPARATOR, MF_STRING, SM_CXSMICON, SM_CXSIZE,
    SM_CYCAPTION, SPI_GETHIGHCONTRAST, SPI_GETNONCLIENTMETRICS, SWP_NOACTIVATE, SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE,
    SW_SHOWNOACTIVATE, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    TPM_TOPALIGN, WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_EXITMENULOOP, WM_INITMENUPOPUP, WM_LBUTTONDOWN,
    WM_MOUSEMOVE, WM_MOVE, WM_NCACTIVATE, WM_NULL, WM_PAINT, WM_SETTINGCHANGE, WM_SHOWWINDOW, WM_SIZE,
    WM_WINDOWPOSCHANGED, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
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
const OPEN_MENU: u32 = 0x8001;
const WM_MOUSELEAVE: u32 = 675;

static INSTALLED: AtomicBool = AtomicBool::new(false);
static INSTALLING: AtomicBool = AtomicBool::new(false);
static PLACING: AtomicBool = AtomicBool::new(false);
static OWNER: AtomicUsize = AtomicUsize::new(0);
static MENU: AtomicUsize = AtomicUsize::new(0);
static POPUP: AtomicUsize = AtomicUsize::new(0);
static PREV_PROC: AtomicUsize = AtomicUsize::new(0);
static MENU_W: AtomicI32 = AtomicI32::new(0);
static COVER_W: AtomicI32 = AtomicI32::new(0);
static CACHED_DPI: AtomicU32 = AtomicU32::new(0);
static LAST_X: AtomicI32 = AtomicI32::new(i32::MIN);
static LAST_Y: AtomicI32 = AtomicI32::new(i32::MIN);
static LAST_W: AtomicI32 = AtomicI32::new(0);
static LAST_H: AtomicI32 = AtomicI32::new(0);
static CAPTION_BG: AtomicU32 = AtomicU32::new(0);
static CAPTION_INK: AtomicU32 = AtomicU32::new(0);
static HOT: AtomicBool = AtomicBool::new(false);
static OPEN: AtomicBool = AtomicBool::new(false);
static LAST_CLOSE_MS: AtomicU64 = AtomicU64::new(0);

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

    let popup = match CreatePopupMenu() {
        Ok(menu) => menu,
        Err(_) => return false,
    };
    fill_popup(popup);
    allow_dark_menus();
    store_fallback_colors();

    let dpi = window_dpi(owner);
    let (width, cover) = measure(dpi);
    MENU_W.store(width, Ordering::Relaxed);
    COVER_W.store(cover, Ordering::Relaxed);
    CACHED_DPI.store(dpi, Ordering::Relaxed);

    let menu = match CreateWindowExW(
        WS_EX_TOOLWINDOW,
        class_name,
        w!(""),
        WS_POPUP,
        0,
        0,
        width,
        8,
        owner,
        None,
        instance,
        None,
    ) {
        Ok(hwnd) => hwnd,
        Err(_) => {
            let _ = DestroyMenu(popup);
            return false;
        }
    };
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
        let _ = DestroyMenu(popup);
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
        let (width, cover) = measure(dpi);
        MENU_W.store(width, Ordering::Relaxed);
        COVER_W.store(cover, Ordering::Relaxed);
    }

    let width = MENU_W.load(Ordering::Relaxed).max(1);
    let Some((x, y, w, h)) = caption_slot(owner, width) else {
        return;
    };
    refresh_caption_color(owner, x, y, w, h);
    move_menu(owner, menu, x, y, w, h);
}

unsafe fn move_menu(owner: HWND, menu: HWND, x: i32, y: i32, w: i32, h: i32) {
    let _ = owner;
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

unsafe fn caption_slot(owner: HWND, menu_width: i32) -> Option<(i32, i32, i32, i32)> {
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
        let caption = GetSystemMetricsForDpi(SM_CYCAPTION, dpi).max(1);
        let button = GetSystemMetricsForDpi(SM_CXSIZE, dpi).max(16);
        let width = (window.right - window.left).max(button * 3);
        (width - button * 3, 0, caption)
    };
    // Fill the caption row. A shorter bar reads as a button sitting on it.
    let bounds = crate::options::title_bar_menu_bounds(
        window.left,
        window.top,
        visible.left,
        buttons_left,
        buttons_top,
        buttons_height,
        icon,
        menu_width,
        buttons_height.max(1),
    );
    if bounds.2 <= 0 {
        return None;
    }
    Some(bounds)
}

unsafe fn measure(dpi: u32) -> (i32, i32) {
    let scale = |px: i32| px * dpi as i32 / 96;
    let fallback_item = scale(72);
    let fallback_cover = scale(36);
    let Some((item, cover)) = text_widths(dpi) else {
        return (fallback_item, fallback_cover);
    };
    (item.max(cover).max(1), cover.max(1))
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

unsafe fn refresh_caption_color(owner: HWND, menu_x: i32, menu_y: i32, menu_w: i32, menu_h: i32) {
    let bg = sample_caption(owner, menu_x, menu_y, menu_w, menu_h).unwrap_or_else(|| CAPTION_BG.load(Ordering::Relaxed));
    if bg == 0 && CAPTION_BG.load(Ordering::Relaxed) == 0 {
        return;
    }
    let active = caption_is_active(owner);
    let ink = crate::options::title_bar_menu_ink(bg, active);
    let changed = bg != CAPTION_BG.load(Ordering::Relaxed) || ink != CAPTION_INK.load(Ordering::Relaxed);
    if changed {
        CAPTION_BG.store(bg, Ordering::Relaxed);
        CAPTION_INK.store(ink, Ordering::Relaxed);
    }
    let menu = hwnd_of(MENU.load(Ordering::Acquire));
    if menu.is_invalid() {
        return;
    }
    if changed {
        let color = bg;
        let _ = DwmSetWindowAttribute(menu, DWMWA_BORDER_COLOR, &color as *const u32 as *const _, 4);
        invalidate(menu);
    }
}

unsafe fn sample_caption(owner: HWND, menu_x: i32, menu_y: i32, menu_w: i32, menu_h: i32) -> Option<u32> {
    let mut window = RECT::default();
    GetWindowRect(owner, &mut window).ok()?;
    let mut buttons = RECT::default();
    if DwmGetWindowAttribute(
        owner,
        DWMWA_CAPTION_BUTTON_BOUNDS,
        &mut buttons as *mut RECT as *mut _,
        std::mem::size_of::<RECT>() as u32,
    )
    .is_err()
    || buttons.right <= buttons.left
    {
        return None;
    }
    let y = window.top + buttons.top + (buttons.bottom - buttons.top) / 2;
    let right = window.left + buttons.left - 8;
    let menu_right = menu_x + menu_w;
    let menu_bottom = menu_y + menu_h;
    let dc = GetDC(None);
    if dc.is_invalid() {
        return None;
    }
    let mut found = [0u32; 4];
    let mut count = 0usize;
    for x in [right - 24, right - 12, right] {
        if x <= menu_right + 4 && y >= menu_y && y < menu_bottom {
            continue;
        }
        if x <= window.left || x >= window.right {
            continue;
        }
        let color = GetPixel(dc, x, y);
        if color.0 != u32::MAX {
            found[count] = color.0;
            count += 1;
        }
    }
    let _ = ReleaseDC(None, dc);
    if count == 0 {
        return None;
    }
    let mut ranked = found[..count].to_vec();
    ranked.sort_by_key(|color| {
        let red = color & 0xff;
        let green = (color >> 8) & 0xff;
        let blue = (color >> 16) & 0xff;
        red * 3 + green * 6 + blue
    });
    Some(ranked[ranked.len() / 2])
}

fn caption_is_active(owner: HWND) -> bool {
    let foreground = unsafe { GetForegroundWindow() };
    if foreground == owner {
        return true;
    }
    let menu = hwnd_of(MENU.load(Ordering::Acquire));
    !menu.is_invalid() && foreground == menu
}

fn store_fallback_colors() {
    let bg = if apps_use_dark() { 0x0020_2020 } else { 0x00ff_ffff };
    CAPTION_BG.store(bg, Ordering::Relaxed);
    CAPTION_INK.store(crate::options::title_bar_menu_ink(bg, true), Ordering::Relaxed);
}

unsafe fn invalidate(hwnd: HWND) {
    let _ = InvalidateRect(hwnd, None, BOOL(0));
}

unsafe fn open_menu(hwnd: HWND) {
    if OPEN.swap(true, Ordering::AcqRel) {
        return;
    }
    let now = unix_ms();
    let last = LAST_CLOSE_MS.load(Ordering::Relaxed);
    if last != 0 && now.saturating_sub(last) < 250 {
        OPEN.store(false, Ordering::Release);
        return;
    }
    let popup_bits = POPUP.load(Ordering::Acquire);
    if popup_bits == 0 {
        OPEN.store(false, Ordering::Release);
        return;
    }
    let popup = HMENU(popup_bits as *mut core::ffi::c_void);
    fill_popup(popup);
    allow_dark_menus();
    let mut rect = RECT::default();
    if GetWindowRect(hwnd, &mut rect).is_err() {
        OPEN.store(false, Ordering::Release);
        return;
    }
    invalidate(hwnd);
    let _ = SetForegroundWindow(hwnd);
    let picked = TrackPopupMenu(
        popup,
        TPM_LEFTALIGN | TPM_TOPALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY,
        rect.left,
        rect.bottom,
        0,
        hwnd,
        None,
    );
    let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
    OPEN.store(false, Ordering::Release);
    LAST_CLOSE_MS.store(unix_ms(), Ordering::Relaxed);
    HOT.store(false, Ordering::Relaxed);
    invalidate(hwnd);
    if let Some(command) = option_from_id(picked.0 as usize) {
        emit(command);
    }
}

fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

unsafe fn fill_caption(hwnd: HWND, hdc: HDC) {
    let mut rect = RECT::default();
    if GetClientRect(hwnd, &mut rect).is_err() {
        return;
    }
    let brush = CreateSolidBrush(COLORREF(CAPTION_BG.load(Ordering::Relaxed)));
    if !brush.is_invalid() {
        let _ = FillRect(hdc, &rect, brush);
        let _ = DeleteObject(brush);
    }
}

unsafe fn paint_menu(hwnd: HWND, hdc: HDC) {
    let mut rect = RECT::default();
    if GetClientRect(hwnd, &mut rect).is_err() || rect.right <= rect.left || rect.bottom <= rect.top {
        return;
    }
    fill_caption(hwnd, hdc);
    let bg = CAPTION_BG.load(Ordering::Relaxed);
    let hot = HOT.load(Ordering::Relaxed) || OPEN.load(Ordering::Relaxed);
    let dpi = window_dpi(hwnd);
    let pad = (8 * dpi as i32 / 96).max(6);
    if hot {
        let hover = crate::options::title_bar_menu_hover(bg);
        let wash = CreateSolidBrush(COLORREF(hover));
        if !wash.is_invalid() {
            let inset = ((rect.bottom - rect.top) / 5).max(1);
            let hover_rect = RECT {
                left: 1,
                top: rect.top + inset,
                right: (pad + text_pixel_width(hdc, dpi, WINDOW_MENU_LABEL).unwrap_or(rect.right) + pad).min(rect.right - 1),
                bottom: rect.bottom - inset,
            };
            if hover_rect.right > hover_rect.left {
                let _ = FillRect(hdc, &hover_rect, wash);
            }
            let _ = DeleteObject(wash);
        }
    }
    let _ = SetBkMode(hdc, TRANSPARENT);
    let _ = SetTextColor(hdc, COLORREF(CAPTION_INK.load(Ordering::Relaxed)));
    if let Some(font) = menu_font(dpi) {
        let previous = SelectObject(hdc, font);
        let mut chars = wide_chars(WINDOW_MENU_LABEL);
        let mut text = rect;
        text.left += pad;
        text.right -= 2;
        let _ = DrawTextW(hdc, &mut chars, &mut text, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
        if !previous.is_invalid() {
            SelectObject(hdc, previous);
        }
        let _ = DeleteObject(font);
    }
}

unsafe fn text_pixel_width(dc: HDC, dpi: u32, text: &str) -> Option<i32> {
    let font = menu_font(dpi)?;
    let previous = SelectObject(dc, font);
    let chars = wide_chars(text);
    let mut size = SIZE::default();
    let ok = GetTextExtentPoint32W(dc, &chars, &mut size).as_bool();
    if !previous.is_invalid() {
        SelectObject(dc, previous);
    }
    let _ = DeleteObject(font);
    if ok && size.cx > 0 { Some(size.cx) } else { None }
}

unsafe fn menu_font(dpi: u32) -> Option<windows::Win32::Graphics::Gdi::HFONT> {
    let mut metrics = windows::Win32::UI::WindowsAndMessaging::NONCLIENTMETRICSW::default();
    metrics.cbSize = std::mem::size_of_val(&metrics) as u32;
    SystemParametersInfoW(
        SPI_GETNONCLIENTMETRICS,
        metrics.cbSize,
        Some(&mut metrics as *mut _ as *mut _),
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
    )
    .ok()?;
    let mut font = metrics.lfMenuFont;
    // Grayscale edges. ClearType fringes if the sampled caption is a shade off.
    font.lfQuality = windows::Win32::Graphics::Gdi::FONT_QUALITY(4);
    let system = GetDpiForSystem().max(96);
    if dpi != 0 && dpi != system && font.lfHeight != 0 {
        font.lfHeight = font.lfHeight.saturating_mul(dpi as i32) / system as i32;
    }
    let created = CreateFontIndirectW(&font);
    if created.is_invalid() { None } else { Some(created) }
}

unsafe extern "system" fn menu_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => {
            let hdc = HDC(wparam.0 as *mut core::ffi::c_void);
            if !hdc.is_invalid() {
                fill_caption(hwnd, hdc);
            }
            return LRESULT(1);
        }
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut paint);
            if !hdc.is_invalid() {
                paint_menu(hwnd, hdc);
            }
            let _ = EndPaint(hwnd, &paint);
            return LRESULT(0);
        }
        WM_MOUSEMOVE => {
            if !HOT.swap(true, Ordering::AcqRel) {
                let mut track = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                let _ = TrackMouseEvent(&mut track);
                invalidate(hwnd);
            }
            return LRESULT(0);
        }
        WM_MOUSELEAVE => {
            HOT.store(false, Ordering::Relaxed);
            invalidate(hwnd);
            return LRESULT(0);
        }
        WM_LBUTTONDOWN => {
            let _ = PostMessageW(hwnd, OPEN_MENU, WPARAM(0), LPARAM(0));
            return LRESULT(0);
        }
        OPEN_MENU => {
            open_menu(hwnd);
            return LRESULT(0);
        }
        WM_INITMENUPOPUP => {
            let opened = wparam.0 as *mut core::ffi::c_void;
            let popup = POPUP.load(Ordering::Acquire) as *mut core::ffi::c_void;
            if !popup.is_null() && opened == popup {
                fill_popup(HMENU(popup));
            }
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
            allow_dark_menus();
            let menu = hwnd_of(MENU.load(Ordering::Acquire));
            if !menu.is_invalid() {
                allow_dark_for_window(menu);
                store_fallback_colors();
                place();
            }
        }
        WM_DESTROY => {
            let menu = hwnd_of(MENU.swap(0, Ordering::AcqRel));
            let popup = HMENU(POPUP.swap(0, Ordering::AcqRel) as *mut core::ffi::c_void);
            OWNER.store(0, Ordering::Release);
            INSTALLED.store(false, Ordering::Release);
            if !popup.is_invalid() {
                let _ = DestroyMenu(popup);
            }
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

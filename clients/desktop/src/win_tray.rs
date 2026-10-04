//! Notification-area icon that can take a file drop.
//!
//! Explorer owns the painted icon, so a normal window cannot be the drop target.
//! While a drag moves onto the icon with the button already down, a layered
//! window is placed over the icon and registered as an OLE drop target. A click
//! that starts on the icon never shows that window, so the icon still receives it.

use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use windows::core::{implement, w, PCWSTR, HRESULT};
use windows::Win32::Foundation::{
    BOOL, COLORREF, ERROR_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, POINT, POINTL, RECT,
    WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, DeleteObject, GetDC, ReleaseDC, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
};
use windows::Win32::System::Com::{IDataObject, DVASPECT_CONTENT, FORMATETC, STGMEDIUM, TYMED_HGLOBAL};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Ole::{
    IDropTarget, IDropTarget_Impl, OleInitialize, RegisterDragDrop, ReleaseStgMedium, RevokeDragDrop, CF_HDROP,
    DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_NONE,
};
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Shell::{
    DragQueryFileW, Shell_NotifyIconGetRect, Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE,
    NIM_MODIFY, NIM_SETVERSION, NOTIFYICONDATAW, NOTIFYICONIDENTIFIER, NOTIFYICON_VERSION_4, NOTIFY_ICON_DATA_FLAGS,
    NOTIFY_ICON_MESSAGE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CallNextHookEx, CreateIconIndirect, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
    MF_CHECKED,
    DispatchMessageW, GetCursorPos, GetMessageW, PostMessageW, PostQuitMessage, RegisterClassW,
    SetForegroundWindow, SetLayeredWindowAttributes, SetWindowPos, SetWindowsHookExW, ShowWindow, TrackPopupMenu,
    TranslateMessage, UnhookWindowsHookEx, DestroyIcon, HHOOK, HICON, HMENU, ICONINFO, LWA_ALPHA, MB_ICONINFORMATION,
    MB_OK, MF_STRING, MSG, MSLLHOOKSTRUCT, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, TPM_BOTTOMALIGN, TPM_NONOTIFY,
    TPM_RETURNCMD, TPM_RIGHTALIGN, WH_MOUSE_LL, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_CONTEXTMENU,
    WM_DESTROY, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP, WM_USER, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};

use crate::icon;
use crate::tray::{TrayAction, TrayPorts};

const CALLBACK: u32 = WM_APP + 1;
const TIP_MSG: u32 = WM_APP + 2;
const QUIT_MSG: u32 = WM_APP + 3;
const ID_OPEN: usize = 1;
const ID_HELLO: usize = 2;
const ID_PIN: usize = 3;
const ID_SIGNOUT: usize = 4;
const ID_CLOSE: usize = 5;
const ID_QUIT: usize = 6;
const ID_CHANGE_PIN: usize = 7;
const ID_PASSWORD: usize = 8;

/// Window and icon handles are thread-safe values. The tray thread creates them
/// and the UI thread only posts messages to the window.
#[derive(Clone, Copy)]
struct SendHandle(usize);
unsafe impl Send for SendHandle {}
unsafe impl Sync for SendHandle {}

fn pack_hwnd(hwnd: HWND) -> SendHandle {
    SendHandle(hwnd.0 as usize)
}
fn unpack_hwnd(handle: SendHandle) -> HWND {
    HWND(handle.0 as *mut std::ffi::c_void)
}
fn pack_icon(icon: HICON) -> SendHandle {
    SendHandle(icon.0 as usize)
}
fn unpack_icon(handle: SendHandle) -> HICON {
    HICON(handle.0 as *mut std::ffi::c_void)
}
fn pack_hook(hook: HHOOK) -> SendHandle {
    SendHandle(hook.0 as usize)
}

struct Shared {
    tx: Sender<TrayAction>,
    wake: Box<dyn Fn() + Send>,
    message: SendHandle,
    overlay: SendHandle,
    icon: SendHandle,
    was_inside: bool,
    drag: bool,
    last_open: Option<Instant>,
}

static SHARED: Mutex<Option<Shared>> = Mutex::new(None);
static HOOK: Mutex<Option<SendHandle>> = Mutex::new(None);
static TIP: Mutex<String> = Mutex::new(String::new());

pub fn start(wake: impl Fn() + Send + 'static) -> Result<TrayPorts, String> {
    claim_single_instance()?;
    let (tx, rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel();
    let thread_tx = tx.clone();
    let thread = std::thread::spawn(move || {
        if let Err(error) = tray_thread(thread_tx, wake, ready_tx.clone()) {
            let _ = ready_tx.send(Err(error));
        }
    });
    let hwnd = ready_rx
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| "The notification icon didn't start.".to_string())?
        .map_err(|error| error)?;
    let tooltip_hwnd = hwnd;
    Ok(TrayPorts {
        events: rx,
        set_tooltip: Box::new(move |text| {
            if let Ok(mut tip) = TIP.lock() {
                *tip = text;
            }
            unsafe {
                let _ = PostMessageW(unpack_hwnd(tooltip_hwnd), TIP_MSG, WPARAM(0), LPARAM(0));
            }
        }),
        shutdown: Box::new(move || {
            unsafe {
                let _ = PostMessageW(unpack_hwnd(hwnd), QUIT_MSG, WPARAM(0), LPARAM(0));
            }
            let _ = thread.join();
        }),
    })
}

pub fn report_error(message: &str) {
    let wide: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
            None,
            PCWSTR(wide.as_ptr()),
            w!("Drop"),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

fn claim_single_instance() -> Result<(), String> {
    unsafe {
        let handle = windows::Win32::System::Threading::CreateMutexW(None, BOOL(1), w!("Local\\KieferMenardDrop"))
            .map_err(|error| error.to_string())?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
                None,
                w!("Drop is already running in the notification area."),
                w!("Drop"),
                MB_OK | MB_ICONINFORMATION,
            );
            std::process::exit(0);
        }
        let _ = handle;
    }
    Ok(())
}

fn tray_thread(tx: Sender<TrayAction>, wake: impl Fn() + Send + 'static, ready: Sender<Result<SendHandle, String>>) -> Result<(), String> {
    unsafe {
        if OleInitialize(None).is_err() {
            let _ = ready.send(Err("Couldn't start drag and drop.".into()));
            return Err("Couldn't start drag and drop.".into());
        }
        let module = GetModuleHandleW(None).map_err(|error| error.to_string())?;
        let instance = HINSTANCE(module.0);
        let class_name = w!("DropTrayWindow");
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: class_name,
            ..Default::default()
        };
        RegisterClassW(&class);
        let message = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            w!("Drop"),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            None,
            None,
            instance,
            None,
        )
        .map_err(|error| error.to_string())?;
        let overlay_class = w!("DropTrayOverlay");
        let overlay_wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: overlay_class,
            ..Default::default()
        };
        RegisterClassW(&overlay_wc);
        let overlay = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED,
            overlay_class,
            w!("Drop drop target"),
            WS_POPUP,
            0,
            0,
            10,
            10,
            None,
            None,
            instance,
            None,
        )
        .map_err(|error| error.to_string())?;
        let _ = SetLayeredWindowAttributes(overlay, COLORREF(0), 1, LWA_ALPHA);
        let target: IDropTarget = FileDrop { overlay }.into();
        if RegisterDragDrop(overlay, &target).is_err() {
            let _ = ready.send(Err("Couldn't register the drop target.".into()));
            return Err("Couldn't register the drop target.".into());
        }
        // Keep the COM object alive for the thread.
        std::mem::forget(target);

        let rgba = clipboard_icon(32);
        let icon = icon_from_rgba(32, 32, &rgba).map_err(|error| error.to_string())?;
        let mut data = notify_data(message, icon, "Drop — drop a file here");
        if !Shell_NotifyIconW(NIM_ADD, &data).as_bool() {
            let _ = ready.send(Err("Couldn't add the notification icon.".into()));
            return Err("Couldn't add the notification icon.".into());
        }
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        let _ = Shell_NotifyIconW(NIM_SETVERSION, &data);

        let hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), instance, 0).map_err(|error| error.to_string())?;
        *HOOK.lock().expect("hook") = Some(pack_hook(hook));
        *SHARED.lock().expect("tray") = Some(Shared {
            tx,
            wake: Box::new(wake),
            message: pack_hwnd(message),
            overlay: pack_hwnd(overlay),
            icon: pack_icon(icon),
            was_inside: false,
            drag: false,
            last_open: None,
        });
        let _ = ready.send(Ok(pack_hwnd(message)));

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = UnhookWindowsHookEx(hook);
        let _ = RevokeDragDrop(overlay);
        let data = notify_data(message, icon, "");
        let _ = Shell_NotifyIconW(NIM_DELETE, &data);
        let _ = DestroyIcon(icon);
        Ok(())
    }
}

fn notify_data(hwnd: HWND, icon: HICON, tip: &str) -> NOTIFYICONDATAW {
    let mut data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: CALLBACK,
        hIcon: icon,
        ..Default::default()
    };
    write_utf16(&mut data.szTip, tip);
    data
}

fn write_utf16(buf: &mut [u16], text: &str) {
    buf.fill(0);
    for (index, unit) in text.encode_utf16().take(buf.len().saturating_sub(1)).enumerate() {
        buf[index] = unit;
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == CALLBACK {
        // NOTIFYICON_VERSION_4 packs the mouse message in the low word.
        on_icon_event((lparam.0 as u32) & 0xFFFF);
        return LRESULT(0);
    }
    if msg == TIP_MSG {
        apply_tip();
        return LRESULT(0);
    }
    if msg == QUIT_MSG {
        PostQuitMessage(0);
        return LRESULT(0);
    }
    if msg == WM_DESTROY {
        return LRESULT(0);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn on_icon_event(event: u32) {
    let mut guard = SHARED.lock().expect("tray");
    let Some(shared) = guard.as_mut() else {
        return;
    };
    match event {
        NIN_SELECT | NIN_KEYSELECT | WM_LBUTTONUP => {
            let now = Instant::now();
            if shared.last_open.is_some_and(|then| now.duration_since(then) < Duration::from_millis(300)) {
                return;
            }
            shared.last_open = Some(now);
            let _ = shared.tx.send(TrayAction::Open);
            (shared.wake)();
        }
        WM_CONTEXTMENU | WM_RBUTTONUP => {
            let tx = shared.tx.clone();
            let wake = shared.wake.as_ref() as *const dyn Fn();
            let hwnd = unpack_hwnd(shared.message);
            drop(guard);
            if let Some(action) = popup_menu(hwnd) {
                let mut guard = SHARED.lock().expect("tray");
                if let Some(shared) = guard.as_mut() {
                    let _ = shared.tx.send(action);
                    (shared.wake)();
                }
                let _ = (tx, wake);
            }
        }
        _ => {}
    }
}

fn clipboard_icon(size: u32) -> Vec<u8> {
    let mut rgba = icon::menu_bar_rgba(size);
    for pixel in rgba.chunks_mut(4) {
        if pixel[3] == 0 {
            continue;
        }
        pixel[0] = 0x1d;
        pixel[1] = 0x68;
        pixel[2] = 0x43;
    }
    rgba
}

fn popup_menu(hwnd: HWND) -> Option<TrayAction> {
    unsafe {
        let menu = CreatePopupMenu().ok()?;
        let _ = AppendMenuW(menu, MF_STRING, ID_OPEN, w!("Open"));
        if crate::biometric::available() {
            let flags = if crate::biometric::enrolled() { MF_STRING | MF_CHECKED } else { MF_STRING };
            let _ = AppendMenuW(menu, flags, ID_HELLO, w!("Windows Hello"));
        }
        if crate::pin::enrolled() {
            let _ = AppendMenuW(menu, MF_STRING | MF_CHECKED, ID_PIN, w!("PIN"));
            let _ = AppendMenuW(menu, MF_STRING, ID_CHANGE_PIN, w!("Change PIN"));
        } else {
            let _ = AppendMenuW(menu, MF_STRING, ID_PIN, w!("Set PIN"));
        }
        let _ = AppendMenuW(menu, MF_STRING, ID_PASSWORD, w!("Change password"));
        let _ = AppendMenuW(menu, MF_STRING, ID_SIGNOUT, w!("Sign out"));
        let close_flags = if crate::window_prefs::close_to_menu_bar() {
            MF_STRING | MF_CHECKED
        } else {
            MF_STRING
        };
        let _ = AppendMenuW(menu, close_flags, ID_CLOSE, w!("Close to notification area"));
        let _ = AppendMenuW(menu, MF_STRING, ID_QUIT, w!("Quit"));
        let mut point = POINT::default();
        let _ = GetCursorPos(&mut point);
        let _ = SetForegroundWindow(hwnd);
        let picked = TrackPopupMenu(
            menu,
            TPM_RIGHTALIGN | TPM_BOTTOMALIGN | TPM_RETURNCMD | TPM_NONOTIFY,
            point.x,
            point.y,
            0,
            hwnd,
            None,
        );
        let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        match picked.0 as usize {
            ID_OPEN => Some(TrayAction::Open),
            ID_HELLO => Some(TrayAction::SetBiometric(!crate::biometric::enrolled())),
            ID_PIN => Some(TrayAction::SetPin(!crate::pin::enrolled())),
            ID_CHANGE_PIN => Some(TrayAction::ChangePin),
            ID_PASSWORD => Some(TrayAction::ChangePassword),
            ID_SIGNOUT => Some(TrayAction::SignOut),
            ID_CLOSE => {
                crate::window_prefs::toggle();
                None
            }
            ID_QUIT => Some(TrayAction::Quit),
            _ => None,
        }
    }
}

fn apply_tip() {
    let tip = TIP.lock().expect("tip").clone();
    let guard = SHARED.lock().expect("tray");
    let Some(shared) = guard.as_ref() else {
        return;
    };
    let data = notify_data(unpack_hwnd(shared.message), unpack_icon(shared.icon), if tip.is_empty() { "Drop" } else { &tip });
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
    }
}

unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        track_drag(info.pt);
    }
    CallNextHookEx(None, code, wparam, lparam)
}

fn track_drag(point: POINT) {
    let mut guard = SHARED.lock().expect("tray");
    let Some(shared) = guard.as_mut() else {
        return;
    };
    let Some(rect) = icon_rect(unpack_hwnd(shared.message)) else {
        return;
    };
    let inside = point.x >= rect.left && point.x < rect.right && point.y >= rect.top && point.y < rect.bottom;
    let down = unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) < 0 };
    let entered = inside && !shared.was_inside && down;
    shared.was_inside = inside;
    if entered {
        show_overlay(unpack_hwnd(shared.overlay), rect);
    } else if !down && !shared.drag {
        hide_overlay(unpack_hwnd(shared.overlay));
    }
}

fn icon_rect(hwnd: HWND) -> Option<RECT> {
    unsafe {
        let id = NOTIFYICONIDENTIFIER {
            cbSize: std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32,
            hWnd: hwnd,
            uID: 1,
            ..Default::default()
        };
        let mut rect = Shell_NotifyIconGetRect(&id).ok()?;
        rect.left -= 4;
        rect.top -= 4;
        rect.right += 4;
        rect.bottom += 4;
        Some(rect)
    }
}

fn show_overlay(hwnd: HWND, rect: RECT) {
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

fn hide_overlay(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 1, LWA_ALPHA);
    }
}

fn icon_from_rgba(width: i32, height: i32, rgba: &[u8]) -> windows::core::Result<HICON> {
    unsafe {
        let hdc = GetDC(None);
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let color = CreateDIBSection(hdc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0)?;
        if !bits.is_null() {
            let dest = std::slice::from_raw_parts_mut(bits as *mut u8, (width * height * 4) as usize);
            for index in 0..(width * height) as usize {
                let red = rgba[index * 4] as u16;
                let green = rgba[index * 4 + 1] as u16;
                let blue = rgba[index * 4 + 2] as u16;
                let alpha = rgba[index * 4 + 3] as u16;
                dest[index * 4] = ((blue * alpha) / 255) as u8;
                dest[index * 4 + 1] = ((green * alpha) / 255) as u8;
                dest[index * 4 + 2] = ((red * alpha) / 255) as u8;
                dest[index * 4 + 3] = alpha as u8;
            }
        }
        let mask_bytes = vec![0xffu8; ((width as usize + 15) / 16) * 2 * height as usize];
        let mask = CreateBitmap(width, height, 1, 1, Some(mask_bytes.as_ptr() as *const _));
        if mask.is_invalid() {
            return Err(windows::core::Error::from_win32());
        }
        let info = ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        };
        let icon = CreateIconIndirect(&info)?;
        let _ = DeleteObject(HGDIOBJ(color.0));
        let _ = DeleteObject(HGDIOBJ(mask.0));
        ReleaseDC(None, hdc);
        Ok(icon)
    }
}

#[implement(IDropTarget)]
struct FileDrop {
    overlay: HWND,
}

impl IDropTarget_Impl for FileDrop_Impl {
    fn DragEnter(
        &self,
        data: Option<&IDataObject>,
        _mods: MODIFIERKEYS_FLAGS,
        _pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        unsafe {
            *effect = if data.is_some_and(has_files) {
                DROPEFFECT_COPY
            } else {
                DROPEFFECT_NONE
            };
            let _ = SetLayeredWindowAttributes(self.this.overlay, COLORREF(0), 160, LWA_ALPHA);
            if let Ok(mut guard) = SHARED.lock() {
                if let Some(shared) = guard.as_mut() {
                    shared.drag = true;
                }
            }
        }
        Ok(())
    }

    fn DragOver(
        &self,
        _mods: MODIFIERKEYS_FLAGS,
        _pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        unsafe {
            *effect = DROPEFFECT_COPY;
        }
        Ok(())
    }

    fn DragLeave(&self) -> windows::core::Result<()> {
        hide_overlay(self.this.overlay);
        if let Ok(mut guard) = SHARED.lock() {
            if let Some(shared) = guard.as_mut() {
                shared.drag = false;
            }
        }
        Ok(())
    }

    fn Drop(
        &self,
        data: Option<&IDataObject>,
        _mods: MODIFIERKEYS_FLAGS,
        _pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        unsafe {
            *effect = DROPEFFECT_COPY;
        }
        hide_overlay(self.this.overlay);
        let paths = data.and_then(read_paths).unwrap_or_default();
        if let Ok(mut guard) = SHARED.lock() {
            if let Some(shared) = guard.as_mut() {
                shared.drag = false;
                shared.was_inside = false;
                if !paths.is_empty() {
                    let _ = shared.tx.send(TrayAction::Dropped(paths));
                    (shared.wake)();
                }
            }
        }
        Ok(())
    }
}

fn has_files(data: &IDataObject) -> bool {
    let format = file_format();
    unsafe { data.QueryGetData(&format).is_ok() }
}

fn read_paths(data: &IDataObject) -> Option<Vec<PathBuf>> {
    unsafe {
        let mut medium = data.GetData(&file_format()).ok()?;
        let handle = hdrop_from(&medium)?;
        let count = DragQueryFileW(handle, 0xFFFF_FFFF, None);
        let mut paths = Vec::new();
        for index in 0..count {
            let len = DragQueryFileW(handle, index, None) as usize;
            let mut buf = vec![0u16; len + 1];
            DragQueryFileW(handle, index, Some(&mut buf));
            let end = buf.iter().position(|unit| *unit == 0).unwrap_or(buf.len());
            let path = String::from_utf16_lossy(&buf[..end]);
            if !path.is_empty() {
                paths.push(PathBuf::from(path));
            }
        }
        ReleaseStgMedium(&mut medium);
        Some(paths)
    }
}

fn file_format() -> FORMATETC {
    FORMATETC {
        cfFormat: CF_HDROP.0,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    }
}

fn hdrop_from(medium: &STGMEDIUM) -> Option<windows::Win32::UI::Shell::HDROP> {
    unsafe {
        let handle = medium.u.hGlobal;
        if handle.is_invalid() {
            None
        } else {
            Some(windows::Win32::UI::Shell::HDROP(handle.0))
        }
    }
}

const NIN_SELECT: u32 = WM_USER;
const NIN_KEYSELECT: u32 = WM_USER + 1;

#[allow(dead_code)]
fn _keep_types_imported() {
    let _ = (HRESULT(0), PCWSTR::null(), HMENU::default(), HBITMAP::default());
    let _ = NOTIFY_ICON_DATA_FLAGS::default();
    let _ = NOTIFY_ICON_MESSAGE::default();
}

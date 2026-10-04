//! Menu-bar icon. A file dropped on the icon uploads to the signed-in account.
//! Clicking the icon opens the window. This file is compiled on macOS only.

use std::ffi::CString;
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
use objc2::{class, msg_send, sel};

use crate::icon;
use crate::tray::{TrayAction, TrayPorts};

static ACTIONS: OnceLock<Sender<TrayAction>> = OnceLock::new();
// The wake callback is `Send` but not `Sync`. `OnceLock` in a static requires `Sync`.
static WAKE: OnceLock<Mutex<Box<dyn Fn() + Send>>> = OnceLock::new();
static BUTTON: OnceLock<usize> = OnceLock::new();
static STATUS_ITEM: OnceLock<usize> = OnceLock::new();
static TARGET: OnceLock<usize> = OnceLock::new();

pub fn start(wake: impl Fn() + Send + 'static) -> Result<TrayPorts, String> {
    if !claim_single_instance() {
        std::process::exit(0);
    }
    let (tx, rx) = mpsc::channel();
    let _ = ACTIONS.set(tx);
    let _ = WAKE.set(Mutex::new(Box::new(wake)));
    unsafe { install_status_item()? };
    let tooltip_button = *BUTTON.get().ok_or("Missing menu bar button.")?;
    let status_item = *STATUS_ITEM.get().ok_or("Missing menu bar item.")?;
    Ok(TrayPorts {
        events: rx,
        set_tooltip: Box::new(move |text| unsafe {
            let button = tooltip_button as *mut AnyObject;
            let _: () = msg_send![button, setToolTip: ns_string(&text)];
        }),
        shutdown: Box::new(move || unsafe {
            let item = status_item as *mut AnyObject;
            let bar: *mut AnyObject = msg_send![class!(NSStatusBar), systemStatusBar];
            let _: () = msg_send![bar, removeStatusItem: item];
        }),
    })
}

pub fn activate() {
    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        let _: Bool = msg_send![app, activateIgnoringOtherApps: Bool::YES];
    }
}

fn claim_single_instance() -> bool {
    let Some(dir) = drop_core::default_config_dir() else {
        return true;
    };
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("instance.lock");
    let Ok(file) = std::fs::OpenOptions::new().create(true).read(true).write(true).open(&path) else {
        return true;
    };
    let fd = std::os::unix::io::AsRawFd::as_raw_fd(&file);
    let locked = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) } == 0;
    if locked {
        std::mem::forget(file);
    }
    locked
}

unsafe fn install_status_item() -> Result<(), String> {
    let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
    // NSApplicationActivationPolicyAccessory hides the Dock icon.
    let _: Bool = msg_send![app, setActivationPolicy: 1isize];

    let target_class = status_target_class();
    let target: *mut AnyObject = msg_send![target_class, new];
    if target.is_null() {
        return Err("Couldn't create the menu bar target.".into());
    }
    let retained = Retained::from_raw(target).ok_or("Couldn't retain the menu bar target.")?;
    let target_ptr = Retained::as_ptr(&retained) as usize;
    std::mem::forget(retained);
    let _ = TARGET.set(target_ptr);

    let bar: *mut AnyObject = msg_send![class!(NSStatusBar), systemStatusBar];
    // NSSquareStatusItemLength is -2.
    let item: *mut AnyObject = msg_send![bar, statusItemWithLength: -2.0f64];
    if item.is_null() {
        return Err("Couldn't create the menu bar item.".into());
    }
    let retained_item = Retained::retain(item).ok_or("Couldn't retain the menu bar item.")?;
    let item_ptr = Retained::as_ptr(&retained_item) as *mut AnyObject;
    std::mem::forget(retained_item);
    let _ = STATUS_ITEM.set(item_ptr as usize);

    let button: *mut AnyObject = msg_send![item_ptr, button];
    if button.is_null() {
        return Err("Couldn't create the menu bar button.".into());
    }
    let _ = BUTTON.set(button as usize);
    let target = target_ptr as *mut AnyObject;
    let _: () = msg_send![button, setTarget: target];
    let _: () = msg_send![button, setAction: sel!(statusClicked:)];
    let _: () = msg_send![button, setToolTip: ns_string("Drop")];
    set_template_image(button);
    subclass_button_for_drops(button);
    let types = dragged_types();
    let _: () = msg_send![button, registerForDraggedTypes: types];
    Ok(())
}

unsafe fn set_template_image(button: *mut AnyObject) {
    let png = icon::png_bytes(&icon::menu_bar_rgba(36), 36);
    let data: *mut AnyObject = msg_send![class!(NSData), dataWithBytes: png.as_ptr() length: png.len()];
    let image: *mut AnyObject = msg_send![class!(NSImage), alloc];
    let image: *mut AnyObject = msg_send![image, initWithData: data];
    if image.is_null() {
        let _: () = msg_send![button, setTitle: ns_string("D")];
        return;
    }
    let _: () = msg_send![image, setSize: NSSize { width: 18.0, height: 18.0 }];
    let _: () = msg_send![image, setTemplate: Bool::YES];
    let _: () = msg_send![button, setImage: image];
}

unsafe fn status_target_class() -> &'static AnyClass {
    static CLASS: OnceLock<&'static AnyClass> = OnceLock::new();
    CLASS.get_or_init(|| {
        let mut builder = ClassBuilder::new("DropStatusTarget", class!(NSObject)).expect("DropStatusTarget");
        // Pointer receivers. `&AnyObject` carries a lifetime, and objc2's
        // `MethodImplementation` impl is not higher-ranked over that lifetime.
        builder.add_method(
            sel!(statusClicked:),
            status_clicked as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
        );
        builder.add_method(sel!(menuOpen:), menu_open as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject));
        builder.add_method(
            sel!(menuSignOut:),
            menu_sign_out as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
        );
        builder.add_method(sel!(menuQuit:), menu_quit as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject));
        builder.register()
    })
}

unsafe fn subclass_button_for_drops(button: *mut AnyObject) {
    let class_ptr: *const AnyClass = msg_send![button, class];
    let superclass = class_ptr.as_ref().ok_or("The menu bar button has no class.").expect("button class");
    let mut builder = ClassBuilder::new("DropStatusButton", superclass).expect("DropStatusButton");
    builder.add_method(
        sel!(draggingEntered:),
        dragging_entered as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject) -> usize,
    );
    builder.add_method(
        sel!(draggingUpdated:),
        dragging_entered as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject) -> usize,
    );
    builder.add_method(
        sel!(prepareForDragOperation:),
        prepare_drag as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject) -> Bool,
    );
    builder.add_method(
        sel!(performDragOperation:),
        perform_drag as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject) -> Bool,
    );
    builder.add_method(
        sel!(draggingExited:),
        dragging_exited as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
    );
    builder.add_method(
        sel!(rightMouseUp:),
        right_mouse_up as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
    );
    let class = builder.register();
    let _old = AnyObject::set_class(&*button, class);
}

extern "C" fn status_clicked(_this: *mut AnyObject, _cmd: Sel, _sender: *mut AnyObject) {
    emit(TrayAction::Open);
}

extern "C" fn menu_open(_this: *mut AnyObject, _cmd: Sel, _sender: *mut AnyObject) {
    emit(TrayAction::Open);
}

extern "C" fn menu_sign_out(_this: *mut AnyObject, _cmd: Sel, _sender: *mut AnyObject) {
    emit(TrayAction::SignOut);
}

extern "C" fn menu_quit(_this: *mut AnyObject, _cmd: Sel, _sender: *mut AnyObject) {
    emit(TrayAction::Quit);
}

extern "C" fn dragging_entered(_this: *mut AnyObject, _cmd: Sel, _sender: *mut AnyObject) -> usize {
    1
}

extern "C" fn prepare_drag(_this: *mut AnyObject, _cmd: Sel, _sender: *mut AnyObject) -> Bool {
    Bool::YES
}

extern "C" fn dragging_exited(_this: *mut AnyObject, _cmd: Sel, _sender: *mut AnyObject) {}

extern "C" fn perform_drag(_this: *mut AnyObject, _cmd: Sel, sender: *mut AnyObject) -> Bool {
    let paths = unsafe { paths_from_drag(sender) };
    if paths.is_empty() {
        Bool::NO
    } else {
        emit(TrayAction::Dropped(paths));
        Bool::YES
    }
}

extern "C" fn right_mouse_up(this: *mut AnyObject, _cmd: Sel, event: *mut AnyObject) {
    unsafe {
        let menu: *mut AnyObject = msg_send![class!(NSMenu), alloc];
        let menu: *mut AnyObject = msg_send![menu, init];
        add_item(menu, "Open", sel!(menuOpen:));
        add_item(menu, "Sign out", sel!(menuSignOut:));
        add_item(menu, "Quit Drop", sel!(menuQuit:));
        let _: () = msg_send![class!(NSMenu), popUpContextMenu: menu withEvent: event forView: this];
    }
}

unsafe fn add_item(menu: *mut AnyObject, title: &str, action: Sel) {
    let blank = ns_string("");
    let item: *mut AnyObject = msg_send![class!(NSMenuItem), alloc];
    let item: *mut AnyObject = msg_send![item, initWithTitle: ns_string(title) action: action keyEquivalent: blank];
    if let Some(target) = TARGET.get() {
        let target = *target as *mut AnyObject;
        let _: () = msg_send![item, setTarget: target];
    }
    let _: () = msg_send![menu, addItem: item];
}

unsafe fn paths_from_drag(sender: *mut AnyObject) -> Vec<PathBuf> {
    let pasteboard: *mut AnyObject = msg_send![sender, draggingPasteboard];
    if pasteboard.is_null() {
        return Vec::new();
    }
    let mut paths = filenames(pasteboard);
    if paths.is_empty() {
        paths.extend(file_urls(pasteboard));
    }
    paths
}

unsafe fn filenames(pasteboard: *mut AnyObject) -> Vec<PathBuf> {
    let listed: *mut AnyObject = msg_send![pasteboard, propertyListForType: ns_string("NSFilenamesPboardType")];
    if listed.is_null() {
        return Vec::new();
    }
    let count: usize = msg_send![listed, count];
    let mut paths = Vec::new();
    for index in 0..count {
        let item: *mut AnyObject = msg_send![listed, objectAtIndex: index];
        if let Some(path) = ns_to_string(item) {
            paths.push(PathBuf::from(path));
        }
    }
    paths
}

unsafe fn file_urls(pasteboard: *mut AnyObject) -> Vec<PathBuf> {
    let raw: *mut AnyObject = msg_send![pasteboard, stringForType: ns_string("public.file-url")];
    let Some(text) = ns_to_string(raw) else {
        return Vec::new();
    };
    let url: *mut AnyObject = msg_send![class!(NSURL), URLWithString: ns_string(&text)];
    if url.is_null() {
        return Vec::new();
    }
    let path: *mut AnyObject = msg_send![url, path];
    ns_to_string(path).into_iter().map(PathBuf::from).collect()
}

fn emit(action: TrayAction) {
    if let Some(tx) = ACTIONS.get() {
        let _ = tx.send(action);
    }
    if let Some(wake) = WAKE.get() {
        if let Ok(wake) = wake.lock() {
            wake();
        }
    }
}

unsafe fn ns_string(value: &str) -> *mut AnyObject {
    let c = CString::new(value).unwrap_or_else(|_| CString::new("").unwrap());
    msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()]
}

unsafe fn ns_to_string(value: *mut AnyObject) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let ptr: *const i8 = msg_send![value, UTF8String];
    if ptr.is_null() {
        return None;
    }
    Some(std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned())
}

unsafe fn dragged_types() -> *mut AnyObject {
    let types: *mut AnyObject = msg_send![class!(NSMutableArray), array];
    let _: () = msg_send![types, addObject: ns_string("public.file-url")];
    let _: () = msg_send![types, addObject: ns_string("NSFilenamesPboardType")];
    types
}

#[repr(C)]
#[derive(Clone, Copy)]
struct NSSize {
    width: f64,
    height: f64,
}

unsafe impl objc2::Encode for NSSize {
    const ENCODING: objc2::Encoding = objc2::Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING]);
}

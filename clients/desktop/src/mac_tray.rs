//! Menu-bar icon, plus a Dock icon while the window is open or minimized.
//!
//! Clicking the menu-bar icon opens the window. A file dropped on the menu-bar
//! icon still uploads when macOS delivers the drag. Mission Control takes most
//! drags at the top of the screen, so the Dock icon is the drop target: macOS
//! sends those files as an open-documents Apple Event. Closing the window
//! (back to the menu bar only) removes the Dock icon.
//!
//! Touch ID, Sign out, and Quit live in the application menu (the menu named
//! after the app, immediately to the right of the Apple menu) and in the Dock
//! icon menu. A right-click on the status item has the same commands as an
//! extra. This file is compiled on macOS only.

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
static APP_MENU: OnceLock<usize> = OnceLock::new();
static DOCK_MENU: OnceLock<usize> = OnceLock::new();

pub fn start(wake: impl Fn() + Send + 'static) -> Result<TrayPorts, String> {
    if !claim_single_instance() {
        std::process::exit(0);
    }
    let (tx, rx) = mpsc::channel();
    let _ = ACTIONS.set(tx);
    let _ = WAKE.set(Mutex::new(Box::new(wake)));
    crate::window_prefs::load();
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
    set_dock_icon_visible(true);
}

/// Regular policy shows the Dock icon. Accessory policy removes it and leaves
/// the menu-bar icon. `LSUIElement` is only the state before the first call.
pub fn set_dock_icon_visible(visible: bool) {
    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        if app.is_null() {
            return;
        }
        if visible {
            // NSApplicationActivationPolicyRegular
            let _: Bool = msg_send![app, setActivationPolicy: 0isize];
            let _: () = msg_send![app, unhide: std::ptr::null_mut::<AnyObject>()];
            let _: Bool = msg_send![app, activateIgnoringOtherApps: Bool::YES];
        } else {
            // NSApplicationActivationPolicyAccessory
            let _: Bool = msg_send![app, setActivationPolicy: 1isize];
        }
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
    register_dock_drops(target);
    install_command_menus(target);
    // The window is already open on this first frame, so the Dock icon comes up
    // with it. Hiding the window later switches back to the accessory policy.
    set_dock_icon_visible(true);
    Ok(())
}

/// The application menu and the Dock menu read the checkmark from the keychain.
/// Call this when that state may have changed.
pub fn close_to_menu_bar() -> bool {
    crate::window_prefs::close_to_menu_bar()
}

pub fn refresh_command_menus() {
    unsafe {
        if let Some(menu) = APP_MENU.get() {
            refresh_menu(*menu as *mut AnyObject, true);
        }
        if let Some(menu) = DOCK_MENU.get() {
            refresh_menu(*menu as *mut AnyObject, false);
        }
    }
}

unsafe fn register_dock_drops(target: *mut AnyObject) {
    let manager: *mut AnyObject = msg_send![class!(NSAppleEventManager), sharedAppleEventManager];
    if manager.is_null() {
        return;
    }
    // kCoreEventClass / kAEOpenDocuments. A file dropped on the Dock icon arrives
    // as this event. Info.plist lists public.item at rank None so the drop is
    // accepted and Drop is not offered as the opener for every file.
    let _: () = msg_send![
        manager,
        setEventHandler: target
        andSelector: sel!(handleOpenDocuments:withReplyEvent:)
        forEventClass: u32::from_be_bytes(*b"aevt")
        andEventID: u32::from_be_bytes(*b"odoc")
    ];
}

unsafe fn set_template_image(button: *mut AnyObject) {
    // 36px drawn at 18pt. Template tint follows the menu bar; the shape is the clipboard.
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
        builder.add_method(
            sel!(menuTouchID:),
            menu_touch_id as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
        );
        builder.add_method(sel!(menuPin:), menu_pin as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject));
        builder.add_method(
            sel!(menuChangePin:),
            menu_change_pin as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
        );
        builder.add_method(
            sel!(menuChangePassword:),
            menu_change_password as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
        );
        builder.add_method(
            sel!(menuCloseToMenuBar:),
            menu_close_to_menu_bar as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
        );
        builder.add_method(
            sel!(menuWillOpen:),
            menu_will_open as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
        );
        builder.add_method(
            sel!(handleOpenDocuments:withReplyEvent:),
            handle_open_documents as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject, *mut AnyObject),
        );
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

extern "C" fn menu_touch_id(_this: *mut AnyObject, _cmd: Sel, _sender: *mut AnyObject) {
    let turn_on = !crate::biometric::enrolled();
    emit(TrayAction::SetBiometric(turn_on));
}

extern "C" fn menu_pin(_this: *mut AnyObject, _cmd: Sel, _sender: *mut AnyObject) {
    let turn_on = !crate::pin::enrolled();
    emit(TrayAction::SetPin(turn_on));
}

extern "C" fn menu_change_pin(_this: *mut AnyObject, _cmd: Sel, _sender: *mut AnyObject) {
    emit(TrayAction::ChangePin);
}

extern "C" fn menu_change_password(_this: *mut AnyObject, _cmd: Sel, _sender: *mut AnyObject) {
    emit(TrayAction::ChangePassword);
}

extern "C" fn menu_close_to_menu_bar(_this: *mut AnyObject, _cmd: Sel, _sender: *mut AnyObject) {
    crate::window_prefs::toggle();
}

extern "C" fn menu_will_open(_this: *mut AnyObject, _cmd: Sel, menu: *mut AnyObject) {
    let app_menu = APP_MENU.get().copied() == Some(menu as usize);
    unsafe { refresh_menu(menu, app_menu) };
}

unsafe fn install_command_menus(target: *mut AnyObject) {
    install_application_menu(target);
    install_dock_menu(target);
}

unsafe fn install_application_menu(target: *mut AnyObject) {
    let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
    if app.is_null() {
        return;
    }
    let mut main: *mut AnyObject = msg_send![app, mainMenu];
    if main.is_null() {
        main = msg_send![class!(NSMenu), alloc];
        main = msg_send![main, init];
        let top = make_item("Drop", sel!(menuOpen:), false);
        let _: () = msg_send![main, addItem: top];
        let _: () = msg_send![app, setMainMenu: main];
    }
    let top: *mut AnyObject = msg_send![main, itemAtIndex: 0isize];
    if top.is_null() {
        return;
    }
    let mut app_menu: *mut AnyObject = msg_send![top, submenu];
    if app_menu.is_null() {
        app_menu = msg_send![class!(NSMenu), alloc];
        app_menu = msg_send![app_menu, init];
        let _: () = msg_send![app_menu, setTitle: ns_string("Drop")];
        let _: () = msg_send![top, setSubmenu: app_menu];
    }
    if !menu_has_action(app_menu, sel!(menuSignOut:)) {
        let quit_at = find_action(app_menu, sel!(terminate:)).or_else(|| find_action(app_menu, sel!(menuQuit:)));
        let insert_at = quit_at.unwrap_or_else(|| item_count(app_menu));
        let mut at = insert_at;
        if crate::biometric::available() {
            insert_item(app_menu, "Touch ID", sel!(menuTouchID:), crate::biometric::enrolled(), at);
            at += 1;
        }
        insert_item(app_menu, if crate::pin::enrolled() { "PIN" } else { "Set PIN" }, sel!(menuPin:), crate::pin::enrolled(), at);
        at += 1;
        insert_item(app_menu, "Change PIN", sel!(menuChangePin:), false, at);
        at += 1;
        insert_item(app_menu, "Change password", sel!(menuChangePassword:), false, at);
        at += 1;
        insert_item(app_menu, "Sign out", sel!(menuSignOut:), false, at);
        at += 1;
        insert_item(
            app_menu,
            "Close to menu bar",
            sel!(menuCloseToMenuBar:),
            crate::window_prefs::close_to_menu_bar(),
            at,
        );
    }
    if let Some(index) = find_action(app_menu, sel!(terminate:)) {
        let quit = item_at(app_menu, index);
        let _: () = msg_send![quit, setTarget: target];
        let _: () = msg_send![quit, setAction: sel!(menuQuit:)];
    } else if !menu_has_action(app_menu, sel!(menuQuit:)) {
        let index = item_count(app_menu);
        insert_item(app_menu, "Quit", sel!(menuQuit:), false, index);
        let quit = item_at(app_menu, index);
        let _: () = msg_send![quit, setKeyEquivalent: ns_string("q")];
    }
    let _: () = msg_send![app_menu, setDelegate: target];
    let _ = APP_MENU.set(app_menu as usize);
}

unsafe fn install_dock_menu(target: *mut AnyObject) {
    let menu: *mut AnyObject = msg_send![class!(NSMenu), alloc];
    let menu: *mut AnyObject = msg_send![menu, init];
    add_item(menu, "Open", sel!(menuOpen:), false);
    if crate::biometric::available() {
        add_item(menu, "Touch ID", sel!(menuTouchID:), crate::biometric::enrolled());
    }
    add_item(menu, if crate::pin::enrolled() { "PIN" } else { "Set PIN" }, sel!(menuPin:), crate::pin::enrolled());
    add_item(menu, "Change PIN", sel!(menuChangePin:), false);
    add_item(menu, "Change password", sel!(menuChangePassword:), false);
    add_item(menu, "Sign out", sel!(menuSignOut:), false);
    add_item(menu, "Quit", sel!(menuQuit:), false);
    let _: () = msg_send![menu, setDelegate: target];
    let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
    if !app.is_null() {
        let _: () = msg_send![app, setDockMenu: menu];
    }
    let _ = DOCK_MENU.set(menu as usize);
}

unsafe fn refresh_menu(menu: *mut AnyObject, app_menu: bool) {
    if menu.is_null() {
        return;
    }
    if !menu_has_action(menu, sel!(menuTouchID:)) && crate::biometric::available() {
        let at = find_action(menu, sel!(menuSignOut:))
            .or_else(|| find_action(menu, sel!(menuCloseToMenuBar:)))
            .or_else(|| find_action(menu, sel!(menuQuit:)))
            .or_else(|| find_action(menu, sel!(terminate:)))
            .unwrap_or_else(|| item_count(menu));
        insert_item(menu, "Touch ID", sel!(menuTouchID:), false, at);
    }
    if !menu_has_action(menu, sel!(menuPin:)) {
        let at = find_action(menu, sel!(menuSignOut:))
            .or_else(|| find_action(menu, sel!(menuCloseToMenuBar:)))
            .or_else(|| find_action(menu, sel!(menuQuit:)))
            .or_else(|| find_action(menu, sel!(terminate:)))
            .unwrap_or_else(|| item_count(menu));
        insert_item(menu, if crate::pin::enrolled() { "PIN" } else { "Set PIN" }, sel!(menuPin:), crate::pin::enrolled(), at);
    }
    if !menu_has_action(menu, sel!(menuChangePin:)) {
        let at = find_action(menu, sel!(menuChangePassword:))
            .or_else(|| find_action(menu, sel!(menuSignOut:)))
            .or_else(|| find_action(menu, sel!(menuCloseToMenuBar:)))
            .or_else(|| find_action(menu, sel!(menuQuit:)))
            .or_else(|| find_action(menu, sel!(terminate:)))
            .unwrap_or_else(|| item_count(menu));
        insert_item(menu, "Change PIN", sel!(menuChangePin:), false, at);
    }
    if !menu_has_action(menu, sel!(menuChangePassword:)) {
        let at = find_action(menu, sel!(menuSignOut:))
            .or_else(|| find_action(menu, sel!(menuCloseToMenuBar:)))
            .or_else(|| find_action(menu, sel!(menuQuit:)))
            .or_else(|| find_action(menu, sel!(terminate:)))
            .unwrap_or_else(|| item_count(menu));
        insert_item(menu, "Change password", sel!(menuChangePassword:), false, at);
    }
    if app_menu && !menu_has_action(menu, sel!(menuCloseToMenuBar:)) {
        let at = find_action(menu, sel!(menuQuit:))
            .or_else(|| find_action(menu, sel!(terminate:)))
            .unwrap_or_else(|| item_count(menu));
        insert_item(menu, "Close to menu bar", sel!(menuCloseToMenuBar:), crate::window_prefs::close_to_menu_bar(), at);
    }
    let touch_on = crate::biometric::enrolled();
    let pin_on = crate::pin::enrolled();
    let close_on = crate::window_prefs::close_to_menu_bar();
    let count = item_count(menu);
    for index in 0..count {
        let item = item_at(menu, index);
        if item.is_null() {
            continue;
        }
        if item_action_is(item, sel!(menuTouchID:)) {
            let state: isize = if touch_on { 1 } else { 0 };
            let _: () = msg_send![item, setState: state];
        }
        if item_action_is(item, sel!(menuPin:)) {
            let title = if pin_on { "PIN" } else { "Set PIN" };
            let _: () = msg_send![item, setTitle: ns_string(title)];
            let state: isize = if pin_on { 1 } else { 0 };
            let _: () = msg_send![item, setState: state];
        }
        if item_action_is(item, sel!(menuChangePin:)) {
            let hidden: Bool = if pin_on { Bool::NO } else { Bool::YES };
            let _: () = msg_send![item, setHidden: hidden];
        }
        if item_action_is(item, sel!(menuCloseToMenuBar:)) {
            let state: isize = if close_on { 1 } else { 0 };
            let _: () = msg_send![item, setState: state];
        }
    }
}

unsafe fn menu_has_action(menu: *mut AnyObject, action: Sel) -> bool {
    find_action(menu, action).is_some()
}

unsafe fn find_action(menu: *mut AnyObject, wanted: Sel) -> Option<isize> {
    let count = item_count(menu);
    for index in 0..count {
        let item = item_at(menu, index);
        if item.is_null() {
            continue;
        }
        if item_action_is(item, wanted) {
            return Some(index);
        }
    }
    None
}

/// A separator's action is null. `Sel` cannot represent that, so compare pointers.
unsafe fn item_action_is(item: *mut AnyObject, wanted: Sel) -> bool {
    let ptr: *const std::ffi::c_void = msg_send![item, action];
    !ptr.is_null() && ptr == wanted.as_ptr().cast()
}

unsafe fn item_count(menu: *mut AnyObject) -> isize {
    msg_send![menu, numberOfItems]
}

unsafe fn item_at(menu: *mut AnyObject, index: isize) -> *mut AnyObject {
    msg_send![menu, itemAtIndex: index]
}

unsafe fn insert_item(menu: *mut AnyObject, title: &str, action: Sel, checked: bool, index: isize) {
    let item = make_item(title, action, checked);
    let _: () = msg_send![menu, insertItem: item atIndex: index];
}

extern "C" fn dragging_entered(_this: *mut AnyObject, _cmd: Sel, _sender: *mut AnyObject) -> usize {
    1
}

extern "C" fn prepare_drag(_this: *mut AnyObject, _cmd: Sel, _sender: *mut AnyObject) -> Bool {
    Bool::YES
}

extern "C" fn dragging_exited(_this: *mut AnyObject, _cmd: Sel, _sender: *mut AnyObject) {}

extern "C" fn handle_open_documents(
    _this: *mut AnyObject,
    _cmd: Sel,
    event: *mut AnyObject,
    _reply: *mut AnyObject,
) {
    let paths = unsafe { paths_from_open_event(event) };
    if !paths.is_empty() {
        emit(TrayAction::Dropped(paths));
    }
}

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
        add_item(menu, "Open", sel!(menuOpen:), false);
        if crate::biometric::available() {
            add_item(menu, "Touch ID", sel!(menuTouchID:), crate::biometric::enrolled());
        }
        add_item(menu, if crate::pin::enrolled() { "PIN" } else { "Set PIN" }, sel!(menuPin:), crate::pin::enrolled());
        if crate::pin::enrolled() {
            add_item(menu, "Change PIN", sel!(menuChangePin:), false);
        }
        add_item(menu, "Change password", sel!(menuChangePassword:), false);
        add_item(menu, "Sign out", sel!(menuSignOut:), false);
        add_item(menu, "Quit", sel!(menuQuit:), false);
        let _: () = msg_send![class!(NSMenu), popUpContextMenu: menu withEvent: event forView: this];
    }
}

unsafe fn add_item(menu: *mut AnyObject, title: &str, action: Sel, checked: bool) {
    let item = make_item(title, action, checked);
    let _: () = msg_send![menu, addItem: item];
}

unsafe fn make_item(title: &str, action: Sel, checked: bool) -> *mut AnyObject {
    let blank = ns_string("");
    let item: *mut AnyObject = msg_send![class!(NSMenuItem), alloc];
    let item: *mut AnyObject = msg_send![item, initWithTitle: ns_string(title) action: action keyEquivalent: blank];
    if let Some(target) = TARGET.get() {
        let target = *target as *mut AnyObject;
        let _: () = msg_send![item, setTarget: target];
    }
    // NSControlStateValueOn is 1. The checkmark is the only state indicator.
    let state: isize = if checked { 1 } else { 0 };
    let _: () = msg_send![item, setState: state];
    item
}

unsafe fn paths_from_open_event(event: *mut AnyObject) -> Vec<PathBuf> {
    if event.is_null() {
        return Vec::new();
    }
    // keyDirectObject
    let direct: *mut AnyObject = msg_send![event, paramDescriptorForKeyword: u32::from_be_bytes(*b"----")];
    if direct.is_null() {
        return Vec::new();
    }
    let count: isize = msg_send![direct, numberOfItems];
    let mut paths = Vec::new();
    if count > 0 {
        for index in 1..=count {
            let item: *mut AnyObject = msg_send![direct, descriptorAtIndex: index];
            if let Some(path) = path_from_descriptor(item) {
                paths.push(path);
            }
        }
    } else if let Some(path) = path_from_descriptor(direct) {
        paths.push(path);
    }
    paths
}

unsafe fn path_from_descriptor(item: *mut AnyObject) -> Option<PathBuf> {
    if item.is_null() {
        return None;
    }
    let url: *mut AnyObject = msg_send![item, fileURLValue];
    if url.is_null() {
        return None;
    }
    let path: *mut AnyObject = msg_send![url, path];
    ns_to_string(path).map(PathBuf::from)
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

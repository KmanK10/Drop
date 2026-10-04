use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use drop_core::{
    format_bytes, load_settings, retention_label, save_settings, Account, CopyPayload, ItemSummary, Settings,
};
use eframe::egui::{
    self, Align, Button, CentralPanel, Color32, Context, Frame, Layout, Margin, RichText, ScrollArea, Stroke,
    TextEdit, Vec2,
};
use zeroize::{Zeroize, Zeroizing};

use crate::biometric::{self, BiometricError};
use crate::icon;
use crate::tray::{TrayAction, TrayPorts};
use crate::worker::{self, Command, WorkerEvent};

const SERVER_HINT: &str = "https://drop.example";

/// Same colors as the website. Light stays the cream palette. Dark follows
/// `prefers-color-scheme` through the system theme egui already reports.
#[derive(Clone, Copy)]
struct Palette {
    background: Color32,
    card: Color32,
    field: Color32,
    ink: Color32,
    muted: Color32,
    green: Color32,
    on_green: Color32,
    danger: Color32,
    line: Color32,
    secondary: Color32,
    hover: Color32,
    active: Color32,
    selection: Color32,
}

impl Palette {
    fn light() -> Self {
        Self {
            background: Color32::from_rgb(0xf7, 0xf3, 0xea),
            card: Color32::from_rgb(0xff, 0xfd, 0xf8),
            field: Color32::from_rgb(0xff, 0xfd, 0xf8),
            ink: Color32::from_rgb(0x1c, 0x19, 0x15),
            muted: Color32::from_rgb(0x6d, 0x66, 0x5c),
            green: Color32::from_rgb(0x1d, 0x68, 0x43),
            on_green: Color32::from_rgb(0xf4, 0xff, 0xf7),
            danger: Color32::from_rgb(0x9d, 0x34, 0x1c),
            line: Color32::from_rgb(0xe4, 0xda, 0xc9),
            secondary: Color32::from_rgb(0xef, 0xe7, 0xd8),
            hover: Color32::from_rgb(0xe4, 0xf2, 0xe9),
            active: Color32::from_rgb(0xd7, 0xeb, 0xde),
            selection: Color32::from_rgb(0xcf, 0xe6, 0xd6),
        }
    }

    fn dark() -> Self {
        Self {
            background: Color32::from_rgb(0x12, 0x10, 0x0d),
            card: Color32::from_rgb(0x26, 0x21, 0x1c),
            field: Color32::from_rgb(0x1c, 0x19, 0x16),
            ink: Color32::from_rgb(0xf6, 0xf1, 0xe8),
            muted: Color32::from_rgb(0xd2, 0xc3, 0xb0),
            green: Color32::from_rgb(0x7d, 0xce, 0xa0),
            on_green: Color32::from_rgb(0x10, 0x21, 0x17),
            danger: Color32::from_rgb(0xf0, 0xa0, 0x90),
            line: Color32::from_rgb(0x53, 0x48, 0x38),
            secondary: Color32::from_rgb(0x1c, 0x19, 0x16),
            hover: Color32::from_rgb(0x1e, 0x33, 0x28),
            active: Color32::from_rgb(0x27, 0x42, 0x33),
            selection: Color32::from_rgb(0x2f, 0x5a, 0x40),
        }
    }

    fn for_theme(theme: egui::Theme) -> Self {
        match theme {
            egui::Theme::Dark => Self::dark(),
            egui::Theme::Light => Self::light(),
        }
    }
}

fn colors(ui: &egui::Ui) -> Palette {
    Palette::for_theme(ui.ctx().theme())
}

pub fn run() -> Result<(), String> {
    let settings_path = drop_core::default_config_dir()
        .ok_or_else(|| "Couldn't find a folder for the server address.".to_string())?
        .join("config.json");
    let settings = load_settings(&settings_path);
    let rgba = icon::tray_rgba(64);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([440.0, 720.0])
            .with_min_inner_size([380.0, 520.0])
            .with_title("Drop")
            .with_icon(egui::IconData {
                rgba,
                width: 64,
                height: 64,
            }),
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native(
        "Drop",
        options,
        Box::new(move |cc| {
            apply_style(&cc.egui_ctx);
            Ok(Box::new(DropApp::new(settings, settings_path)))
        }),
    )
    .map_err(|error| error.to_string())
}

struct DropApp {
    settings_path: PathBuf,
    server: String,
    username: String,
    password: String,
    password_epoch: u64,
    draft: String,
    account: Option<Account>,
    items: Vec<ItemSummary>,
    http: bool,
    status: String,
    error: String,
    busy: bool,
    quit: bool,
    pending_drops: Vec<PathBuf>,
    biometrics: bool,
    biometric_available: bool,
    tried_biometrics: bool,
    refresh_biometrics: bool,
    biometric_epoch: u64,
    tx: Sender<Command>,
    ui_tx: Sender<WorkerEvent>,
    rx: Receiver<WorkerEvent>,
    worker: Option<std::thread::JoinHandle<()>>,
    tray: Option<TrayHandle>,
}

struct TrayHandle {
    events: Receiver<TrayAction>,
    set_tooltip: Box<dyn Fn(String) + Send>,
    shutdown: Option<Box<dyn FnOnce() + Send>>,
}

impl Drop for TrayHandle {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            shutdown();
        }
    }
}

impl DropApp {
    fn new(settings: Settings, settings_path: PathBuf) -> Self {
        let (tx, worker_rx) = mpsc::channel();
        let (event_tx, rx) = mpsc::channel();
        let ui_tx = event_tx.clone();
        let worker = worker::spawn(worker_rx, event_tx);
        Self {
            settings_path,
            server: settings.server_url,
            username: settings.username,
            password: String::new(),
            password_epoch: 0,
            draft: String::new(),
            account: None,
            items: Vec::new(),
            http: false,
            status: String::new(),
            error: String::new(),
            busy: false,
            quit: false,
            pending_drops: Vec::new(),
            biometrics: biometric::enrolled(),
            biometric_available: biometric::available(),
            tried_biometrics: false,
            refresh_biometrics: false,
            biometric_epoch: 0,
            tx,
            ui_tx,
            rx,
            worker: Some(worker),
            tray: None,
        }
    }

    fn send(&mut self, command: Command) {
        if self.tx.send(command).is_err() {
            self.error = "Drop stopped responding.".into();
            self.busy = false;
        }
    }

    fn pump(&mut self, ctx: &Context) {
        self.ensure_tray(ctx);
        let actions = if let Some(tray) = &self.tray {
            let mut actions = Vec::new();
            while let Ok(action) = tray.events.try_recv() {
                actions.push(action);
            }
            actions
        } else {
            Vec::new()
        };
        for action in actions {
            self.on_tray(ctx, action);
        }
        while let Ok(event) = self.rx.try_recv() {
            self.on_worker(ctx, event);
        }
    }

    fn ensure_tray(&mut self, ctx: &Context) {
        if self.tray.is_some() {
            return;
        }
        let wake_ctx = ctx.clone();
        let wake = move || wake_ctx.request_repaint();
        match start_tray(wake) {
            Ok(ports) => {
                self.tray = Some(TrayHandle {
                    events: ports.events,
                    set_tooltip: ports.set_tooltip,
                    shutdown: Some(ports.shutdown),
                });
                self.update_tooltip();
            }
            Err(error) => {
                self.error = error;
            }
        }
    }

    fn on_tray(&mut self, ctx: &Context, action: TrayAction) {
        match action {
            TrayAction::Open => show_window(ctx),
            TrayAction::SignOut => self.sign_out(),
            TrayAction::Quit => self.request_quit(ctx),
            #[cfg(target_os = "macos")]
            TrayAction::SetBiometric(enabled) => self.set_biometrics(enabled),
            TrayAction::Dropped(paths) => {
                show_window(ctx);
                if self.account.is_none() {
                    self.pending_drops.extend(paths);
                    self.error = "Sign in before dropping files.".into();
                } else {
                    self.busy = true;
                    self.error.clear();
                    self.send(Command::UploadPaths(paths));
                }
            }
        }
    }

    fn on_worker(&mut self, ctx: &Context, event: WorkerEvent) {
        match event {
            WorkerEvent::Snapshot(snapshot) => {
                self.http = snapshot.http;
                self.account = Some(snapshot.account);
                self.items = snapshot.items;
                self.busy = false;
                self.error.clear();
                self.update_tooltip();
                if self.refresh_biometrics {
                    self.refresh_biometrics = false;
                    self.busy = true;
                    self.send(Command::PrepareBiometric);
                }
                if !self.pending_drops.is_empty() {
                    let paths = std::mem::take(&mut self.pending_drops);
                    self.busy = true;
                    self.send(Command::UploadPaths(paths));
                }
            }
            WorkerEvent::Status(status) => {
                self.status = status;
                if self.status.is_empty() {
                    self.busy = false;
                }
            }
            WorkerEvent::Error(error) => {
                self.refresh_biometrics = false;
                self.biometrics = biometric::enrolled();
                self.error = error;
                self.busy = false;
                self.status.clear();
                show_window(ctx);
            }
            WorkerEvent::BiometricMaterial(bytes) => {
                self.status = format!("{}…", biometric::label());
                self.store_biometrics(bytes);
            }
            WorkerEvent::BiometricUnlocked { epoch, material } => {
                if epoch != self.biometric_epoch {
                    return;
                }
                let server = self.server.trim().to_string();
                let username = self.username.trim().to_string();
                self.http = server.starts_with("http://");
                self.send(Command::Restore {
                    server,
                    username,
                    material,
                });
            }
            WorkerEvent::BiometricStored { epoch } => {
                if epoch != self.biometric_epoch || self.account.is_none() {
                    biometric::delete();
                    if epoch == self.biometric_epoch {
                        self.biometrics = false;
                        self.busy = false;
                    }
                    return;
                }
                self.biometrics = true;
                self.busy = false;
                if !cfg!(target_os = "macos") {
                    self.status = format!("{} is on.", biometric::label());
                }
            }
            WorkerEvent::BiometricCanceled { epoch } => {
                if epoch != self.biometric_epoch {
                    return;
                }
                self.biometrics = biometric::enrolled();
                self.busy = false;
                self.status.clear();
            }
            WorkerEvent::BiometricFailed { epoch, message } => {
                if epoch != self.biometric_epoch {
                    return;
                }
                self.biometrics = biometric::enrolled();
                self.error = message;
                self.busy = false;
                self.status.clear();
            }
            WorkerEvent::SignedOut => {
                self.account = None;
                self.items.clear();
                self.busy = false;
                self.status.clear();
                self.http = self.server.trim().starts_with("http://");
                self.update_tooltip();
                show_window(ctx);
            }
            WorkerEvent::Copy(payload) => {
                self.busy = false;
                match write_clipboard(payload) {
                    Ok(()) => self.status = "Copied.".into(),
                    Err(error) => self.error = error,
                }
            }
            WorkerEvent::Download(mut file) => {
                self.busy = false;
                if let Some(path) = rfd::FileDialog::new().set_file_name(&file.name).save_file() {
                    if let Err(error) = std::fs::write(&path, file.bytes.as_slice()) {
                        self.error = format!("Couldn't save that file. {error}");
                    } else {
                        self.status = format!("Saved {}.", file.name);
                    }
                }
                file.bytes.zeroize();
            }
        }
    }

    fn request_quit(&mut self, ctx: &Context) {
        self.quit = true;
        let _ = self.tx.send(Command::Shutdown {
            keep_session: biometric::enrolled(),
        });
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn set_biometrics(&mut self, enabled: bool) {
        if self.busy {
            return;
        }
        if enabled {
            if self.account.is_none() {
                self.error = "Sign in with your password before turning this on.".into();
                return;
            }
            self.biometrics = true;
            self.busy = true;
            self.error.clear();
            self.send(Command::PrepareBiometric);
        } else {
            self.biometric_epoch = self.biometric_epoch.wrapping_add(1);
            biometric::delete();
            self.biometrics = false;
            self.refresh_biometrics = false;
            if !cfg!(target_os = "macos") {
                self.status = format!("{} is off.", biometric::label());
            }
        }
    }

    fn sign_out(&mut self) {
        self.biometric_epoch = self.biometric_epoch.wrapping_add(1);
        biometric::delete();
        self.biometrics = false;
        self.refresh_biometrics = false;
        self.busy = true;
        self.send(Command::SignOut);
    }

    fn unlock_with_biometrics(&mut self) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error.clear();
        self.status = "Unlocking…".into();
        let epoch = self.biometric_epoch;
        let tx = self.ui_tx.clone();
        // Touch ID and Windows Hello present a system dialog. Blocking the UI
        // thread here keeps that dialog from appearing.
        std::thread::spawn(move || match biometric::load() {
            Ok(material) => {
                let _ = tx.send(WorkerEvent::BiometricUnlocked { epoch, material });
            }
            Err(BiometricError::Canceled) => {
                let _ = tx.send(WorkerEvent::BiometricCanceled { epoch });
            }
            Err(BiometricError::Failed(message)) => {
                let _ = tx.send(WorkerEvent::BiometricFailed { epoch, message });
            }
        });
    }

    fn store_biometrics(&mut self, bytes: Zeroizing<Vec<u8>>) {
        let epoch = self.biometric_epoch;
        let tx = self.ui_tx.clone();
        std::thread::spawn(move || {
            let mut bytes = bytes;
            let result = biometric::store(&bytes);
            bytes.zeroize();
            match result {
                Ok(()) => {
                    let _ = tx.send(WorkerEvent::BiometricStored { epoch });
                }
                Err(BiometricError::Canceled) => {
                    let _ = tx.send(WorkerEvent::BiometricCanceled { epoch });
                }
                Err(BiometricError::Failed(message)) => {
                    let _ = tx.send(WorkerEvent::BiometricFailed { epoch, message });
                }
            }
        });
    }

    fn update_tooltip(&self) {
        let Some(tray) = &self.tray else {
            return;
        };
        let tip = if let Some(account) = &self.account {
            format!("Drop — {}", account.username)
        } else {
            "Drop — sign in".into()
        };
        (tray.set_tooltip)(tip);
    }

    fn submit_sign_in(&mut self) {
        let server = self.server.trim().to_string();
        let username = self.username.trim().to_string();
        if server.is_empty() || username.is_empty() || self.password.is_empty() {
            self.error = "Enter the server, username, and password.".into();
            return;
        }
        let _ = save_settings(
            &self.settings_path,
            &Settings {
                server_url: server.clone(),
                username: username.clone(),
            },
        );
        let password = Zeroizing::new(std::mem::take(&mut self.password));
        self.password_epoch = self.password_epoch.wrapping_add(1);
        self.busy = true;
        self.error.clear();
        self.status = "Signing in…".into();
        self.http = server.starts_with("http://");
        self.refresh_biometrics = biometric::enrolled();
        self.send(Command::SignIn {
            server,
            username,
            password,
        });
    }

    fn sign_in_ui(&mut self, ui: &mut egui::Ui) {
        let colors = colors(ui);
        wordmark(ui);
        ui.add_space(8.0);
        ui.label(RichText::new("Sign in").size(18.0).strong().color(colors.ink));
        ui.label(
            RichText::new("The password unlocks items on this device. It is kept in memory until you quit Drop, and it is not saved.")
                .color(colors.muted)
                .size(13.0),
        );
        ui.add_space(8.0);
        labeled(ui, "Server", &mut self.server, SERVER_HINT);
        labeled(ui, "Username", &mut self.username, "");
        ui.label(RichText::new("Password").color(colors.ink));
        let password_id = egui::Id::new(("drop-password", self.password_epoch));
        ui.add(
            TextEdit::singleline(&mut self.password)
                .password(true)
                .id(password_id)
                .desired_width(f32::INFINITY)
                .hint_text("Password"),
        );
        if self.http || self.server.trim().starts_with("http://") {
            ui.add_space(6.0);
            ui.label(
                RichText::new("This connection is not HTTPS. The password still stays on this device, but the network can see the session.")
                    .color(colors.muted)
                    .size(12.0),
            );
        }
        notice(ui, &self.error, &self.status);
        ui.add_space(8.0);
        let button = ui.add_enabled(
            !self.busy,
            primary_button(if self.busy { "Signing in…" } else { "Sign in" }, &colors),
        );
        if button.clicked() {
            self.submit_sign_in();
        }
        if self.biometrics {
            ui.add_space(8.0);
            let unlock = ui.add_enabled(!self.busy, Button::new(biometric::label()));
            if unlock.clicked() {
                self.unlock_with_biometrics();
            }
        }
        ui.add_space(12.0);
        ui.label(
            RichText::new("Accounts are invite-only. Ask the person who runs this Drop for a username. Closing this window keeps Drop in the tray.")
                .color(colors.muted)
                .size(12.0),
        );
    }

    fn clipboard_ui(&mut self, ui: &mut egui::Ui, #[allow(unused_variables)] ctx: &Context) {
        let account = self.account.clone();
        let Some(account) = account else {
            return;
        };
        ui.horizontal(|ui| {
            wordmark(ui);
            // Mac keeps Sign out, Quit, and Touch ID on the menu-bar menu.
            if !cfg!(target_os = "macos") {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(Button::new("Quit")).clicked() {
                        self.request_quit(ctx);
                    }
                    if ui.add(Button::new("Sign out")).clicked() && !self.busy {
                        self.sign_out();
                    }
                });
            }
        });
        let colors = colors(ui);
        ui.label(
            RichText::new(format!(
                "{} · {} of {} · kept {}",
                account.username,
                format_bytes(account.used_bytes),
                format_bytes(account.quota_bytes),
                retention_label(account.ttl_ms)
            ))
            .color(colors.muted)
            .size(12.0),
        );
        if self.http {
            ui.label(
                RichText::new("This connection is not HTTPS. Items are still encrypted before they are uploaded.")
                    .color(colors.muted)
                    .size(12.0),
            );
        }
        if self.biometric_available && !cfg!(target_os = "macos") {
            let mut enabled = self.biometrics;
            let response = ui.add_enabled(!self.busy, egui::Checkbox::new(&mut enabled, biometric::label()));
            if response.changed() {
                self.set_biometrics(enabled);
            }
            ui.label(
                RichText::new("Off until you turn this on after signing in with your password. The content key goes in the system keychain, not a file, and the password is not stored. Sign out removes it.")
                    .color(colors.muted)
                    .size(11.0),
            );
        }
        ui.add_space(8.0);
        ui.add(
            TextEdit::multiline(&mut self.draft)
                .desired_rows(4)
                .desired_width(f32::INFINITY)
                .hint_text("Paste or write something"),
        );
        ui.horizontal(|ui| {
            if ui.add_enabled(!self.busy, primary_button("Save text", &colors)).clicked() {
                let text = std::mem::take(&mut self.draft);
                self.busy = true;
                self.error.clear();
                self.send(Command::UploadText(text));
            }
            if ui.add_enabled(!self.busy, Button::new("Paste")).clicked() {
                self.paste();
            }
            if ui.add_enabled(!self.busy, Button::new("Upload file…")).clicked() {
                self.pick_files();
            }
        });
        notice(ui, &self.error, &self.status);
        ui.add_space(8.0);
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if self.items.is_empty() {
                ui.label(RichText::new("Nothing here yet. Drop a file on the tray icon, or save a note.").color(colors.muted));
            }
            let items = self.items.clone();
            for item in items {
                self.item_card(ui, &item);
                ui.add_space(8.0);
            }
        });
        ui.add_space(6.0);
        let note = if cfg!(target_os = "macos") {
            "Closing this window keeps Drop in the menu bar.".to_string()
        } else {
            let forget = if self.biometrics {
                "Sign out removes the keychain item."
            } else {
                "Quit forgets the key."
            };
            format!("Closing this window keeps Drop in the notification area. {forget}")
        };
        ui.label(RichText::new(note).color(colors.muted).size(11.0));
    }

    fn item_card(&mut self, ui: &mut egui::Ui, item: &ItemSummary) {
        let colors = colors(ui);
        Frame::none()
            .fill(colors.card)
            .stroke(Stroke::new(1.0_f32, colors.line))
            .inner_margin(Margin::same(10.0))
            .rounding(8.0)
            .show(ui, |ui| {
                ui.label(RichText::new(&item.title).strong().color(colors.ink));
                ui.label(RichText::new(format!("{} · {}", item.detail, item.when)).color(colors.muted).size(12.0));
                ui.push_id(&item.id, |ui| {
                    ui.horizontal(|ui| {
                        if item.can_copy
                            && ui
                                .add_enabled(!self.busy, Button::new(RichText::new("Copy").color(colors.green)))
                                .clicked()
                        {
                            self.busy = true;
                            self.error.clear();
                            self.send(Command::Copy(item.id.clone()));
                        }
                        ui.add_enabled_ui(!self.busy, |ui| {
                            let menu_id = ui.id().with("shortcuts");
                            let mut bar = egui::menu::BarState::load(ui.ctx(), menu_id);
                            let plus = row_mark(ui, RowMark::Plus, colors.green).on_hover_text("Shortcuts");
                            bar.bar_menu(&plus, |ui| {
                                if item.can_copy && ui.button("Copy").clicked() {
                                    ui.close_menu();
                                    self.busy = true;
                                    self.error.clear();
                                    self.send(Command::Copy(item.id.clone()));
                                }
                                if ui.button("Download").clicked() {
                                    ui.close_menu();
                                    self.busy = true;
                                    self.error.clear();
                                    self.send(Command::Download(item.id.clone()));
                                }
                            });
                            bar.store(ui.ctx(), menu_id);
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add_enabled_ui(!self.busy, |ui| {
                                let trash = row_mark(ui, RowMark::Trash, colors.danger).on_hover_text("Delete");
                                if trash.clicked() {
                                    self.busy = true;
                                    self.error.clear();
                                    self.send(Command::Delete(item.id.clone()));
                                }
                            });
                        });
                    });
                });
            });
    }

    fn paste(&mut self) {
        self.error.clear();
        let mut clipboard = match arboard::Clipboard::new() {
            Ok(clipboard) => clipboard,
            Err(_) => {
                self.error = "Couldn't read the clipboard.".into();
                return;
            }
        };
        if let Ok(text) = clipboard.get_text() {
            if !text.trim().is_empty() {
                self.busy = true;
                self.send(Command::UploadText(text));
                return;
            }
        }
        if let Ok(image) = clipboard.get_image() {
            match rgba_to_png(image.width, image.height, &image.bytes) {
                Ok(bytes) => {
                    self.busy = true;
                    self.send(Command::UploadBytes {
                        name: "pasted.png".into(),
                        mime: "image/png".into(),
                        bytes: Zeroizing::new(bytes),
                    });
                }
                Err(error) => self.error = error,
            }
            return;
        }
        self.error = "The clipboard is empty.".into();
    }

    fn pick_files(&mut self) {
        let Some(paths) = rfd::FileDialog::new().pick_files() else {
            return;
        };
        if paths.is_empty() {
            return;
        }
        self.busy = true;
        self.error.clear();
        self.send(Command::UploadPaths(paths));
    }
}

impl eframe::App for DropApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.pump(ctx);
        if self.account.is_none() && !self.tried_biometrics && self.biometrics && !self.busy {
            self.tried_biometrics = true;
            self.unlock_with_biometrics();
        }
        if ctx.input(|input| input.viewport().close_requested()) && !self.quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            // Fully closed: menu bar only. A minimized window stays in the Dock,
            // because minimize does not request close.
            #[cfg(target_os = "macos")]
            crate::mac_tray::set_dock_icon_visible(false);
        }
        CentralPanel::default().show(ctx, |ui| {
            if self.account.is_some() {
                self.clipboard_ui(ui, ctx);
            } else {
                self.sign_in_ui(ui);
            }
        });
        ctx.request_repaint_after(Duration::from_millis(200));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.tx.send(Command::Shutdown {
            keep_session: biometric::enrolled(),
        });
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn show_window(ctx: &Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    #[cfg(target_os = "macos")]
    crate::mac_tray::activate();
}

fn apply_style(ctx: &Context) {
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        let palette = Palette::for_theme(theme);
        let mut style = (*ctx.style_of(theme)).clone();
        style.visuals = paint(theme, palette);
        style.spacing.item_spacing = Vec2::new(8.0, 6.0);
        style.spacing.button_padding = Vec2::new(10.0, 6.0);
        ctx.set_style_of(theme, style);
    }
    ctx.set_theme(egui::ThemePreference::System);
}

fn paint(theme: egui::Theme, palette: Palette) -> egui::Visuals {
    let mut visuals = match theme {
        egui::Theme::Dark => egui::Visuals::dark(),
        egui::Theme::Light => egui::Visuals::light(),
    };
    visuals.window_fill = palette.background;
    visuals.panel_fill = palette.background;
    visuals.extreme_bg_color = palette.field;
    visuals.faint_bg_color = palette.secondary;
    visuals.code_bg_color = palette.card;
    visuals.override_text_color = Some(palette.ink);
    visuals.hyperlink_color = palette.green;
    visuals.warn_fg_color = palette.danger;
    visuals.error_fg_color = palette.danger;
    visuals.selection.bg_fill = palette.selection;
    visuals.selection.stroke.color = palette.green;
    let widgets = &mut visuals.widgets;
    for widget in [
        &mut widgets.noninteractive,
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
        &mut widgets.open,
    ] {
        widget.fg_stroke.color = palette.ink;
        widget.bg_stroke.color = palette.line;
    }
    widgets.noninteractive.bg_fill = palette.card;
    widgets.inactive.bg_fill = palette.card;
    widgets.hovered.bg_fill = palette.hover;
    widgets.active.bg_fill = palette.active;
    widgets.open.bg_fill = palette.card;
    visuals
}

fn wordmark(ui: &mut egui::Ui) {
    let colors = colors(ui);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(32.0), egui::Sense::hover());
        ui.painter().circle_filled(rect.center(), 16.0, colors.green);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "D",
            egui::FontId::proportional(18.0),
            colors.on_green,
        );
        ui.vertical(|ui| {
            ui.label(RichText::new("Drop").size(22.0).strong().color(colors.ink));
            ui.label(RichText::new("Private clipboard").size(11.0).color(colors.muted));
        });
    });
}

fn labeled(ui: &mut egui::Ui, label: &str, value: &mut String, hint: &str) {
    let colors = colors(ui);
    ui.label(RichText::new(label).color(colors.ink));
    let mut edit = TextEdit::singleline(value).desired_width(f32::INFINITY);
    if !hint.is_empty() {
        edit = edit.hint_text(hint);
    }
    ui.add(edit);
}

enum RowMark {
    Plus,
    Trash,
}

/// A circled mark in the same family as the iPhone row buttons.
fn row_mark(ui: &mut egui::Ui, mark: RowMark, ink: Color32) -> egui::Response {
    let size = 28.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let center = rect.center();
        let stroke = Stroke::new(1.5_f32, ink);
        painter.circle_stroke(center, size * 0.5 - 1.0, stroke);
        match mark {
            RowMark::Plus => {
                painter.hline(center.x - 5.0..=center.x + 5.0, center.y, stroke);
                painter.vline(center.x, center.y - 5.0..=center.y + 5.0, stroke);
            }
            RowMark::Trash => {
                painter.hline(center.x - 5.5..=center.x + 5.5, center.y - 4.0, stroke);
                painter.hline(center.x - 2.0..=center.x + 2.0, center.y - 6.2, stroke);
                let body = egui::Rect::from_center_size(center + Vec2::new(0.0, 2.2), Vec2::new(9.0, 8.0));
                painter.rect_stroke(body, egui::Rounding::same(1.0), stroke);
            }
        }
    }
    response
}

fn primary_button<'a>(label: &'a str, colors: &Palette) -> Button<'a> {
    Button::new(RichText::new(label).color(colors.on_green))
        .fill(colors.green)
        .min_size(Vec2::new(0.0, 32.0))
}

fn notice(ui: &mut egui::Ui, error: &str, status: &str) {
    let colors = colors(ui);
    if !error.is_empty() {
        ui.add_space(6.0);
        ui.label(RichText::new(error).color(colors.danger));
    } else if !status.is_empty() {
        ui.add_space(6.0);
        ui.label(RichText::new(status).color(colors.muted));
    }
}

fn write_clipboard(payload: CopyPayload) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|_| "Couldn't copy that.".to_string())?;
    match payload {
        CopyPayload::Text(mut text) => {
            clipboard
                .set_text(text.as_str())
                .map_err(|_| "Couldn't copy that.".to_string())?;
            text.zeroize();
            Ok(())
        }
        CopyPayload::Image { mut bytes, .. } => {
            let image = image::load_from_memory(&bytes).map_err(|_| {
                "Couldn't copy that image. Download it instead.".to_string()
            })?;
            let rgba = image.to_rgba8();
            let width = rgba.width() as usize;
            let height = rgba.height() as usize;
            clipboard
                .set_image(arboard::ImageData {
                    width,
                    height,
                    bytes: std::borrow::Cow::Owned(rgba.into_raw()),
                })
                .map_err(|_| "Couldn't copy that.".to_string())?;
            bytes.zeroize();
            Ok(())
        }
    }
}

fn rgba_to_png(width: usize, height: usize, bytes: &[u8]) -> Result<Vec<u8>, String> {
    let image = image::RgbaImage::from_raw(width as u32, height as u32, bytes.to_vec())
        .ok_or_else(|| "Couldn't read that image.".to_string())?;
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|_| "Couldn't read that image.".to_string())?;
    Ok(png)
}

#[cfg(target_os = "windows")]
fn start_tray(wake: impl Fn() + Send + 'static) -> Result<TrayPorts, String> {
    crate::win_tray::start(wake)
}

#[cfg(target_os = "macos")]
fn start_tray(wake: impl Fn() + Send + 'static) -> Result<TrayPorts, String> {
    crate::mac_tray::start(wake)
}

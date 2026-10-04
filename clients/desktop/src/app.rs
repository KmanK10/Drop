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

use crate::icon;
use crate::tray::{TrayAction, TrayPorts};
use crate::worker::{self, Command, WorkerEvent};

const GREEN: Color32 = Color32::from_rgb(0x1d, 0x68, 0x43);
const CREAM: Color32 = Color32::from_rgb(0xf7, 0xf3, 0xea);
const CARD: Color32 = Color32::from_rgb(0xff, 0xfd, 0xf8);
const INK: Color32 = Color32::from_rgb(0x1c, 0x19, 0x15);
const MUTED: Color32 = Color32::from_rgb(0x6d, 0x66, 0x5c);
const DANGER: Color32 = Color32::from_rgb(0x9d, 0x34, 0x1c);
const LINE: Color32 = Color32::from_rgb(0xe4, 0xda, 0xc9);
const ON_GREEN: Color32 = Color32::from_rgb(0xf4, 0xff, 0xf7);

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
    pending_delete: Option<String>,
    pending_drops: Vec<PathBuf>,
    tx: Sender<Command>,
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
            pending_delete: None,
            pending_drops: Vec::new(),
            tx,
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
            TrayAction::SignOut => {
                self.busy = true;
                self.send(Command::SignOut);
            }
            TrayAction::Quit => self.request_quit(ctx),
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
                self.error = error;
                self.busy = false;
                self.status.clear();
                show_window(ctx);
            }
            WorkerEvent::SignedOut => {
                self.account = None;
                self.items.clear();
                self.pending_delete = None;
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
        let _ = self.tx.send(Command::Shutdown);
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
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
        self.send(Command::SignIn {
            server,
            username,
            password,
        });
    }

    fn sign_in_ui(&mut self, ui: &mut egui::Ui) {
        wordmark(ui);
        ui.add_space(8.0);
        ui.label(RichText::new("Sign in").size(18.0).strong().color(INK));
        ui.label(
            RichText::new("The password unlocks items on this device. It is kept in memory until you quit Drop, and it is not saved.")
                .color(MUTED)
                .size(13.0),
        );
        ui.add_space(8.0);
        labeled(ui, "Server", &mut self.server, false);
        labeled(ui, "Username", &mut self.username, false);
        ui.label(RichText::new("Password").color(INK));
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
                    .color(MUTED)
                    .size(12.0),
            );
        }
        notice(ui, &self.error, &self.status);
        ui.add_space(8.0);
        let button = ui.add_enabled(!self.busy, primary_button(if self.busy { "Signing in…" } else { "Sign in" }));
        if button.clicked() {
            self.submit_sign_in();
        }
        ui.add_space(12.0);
        ui.label(
            RichText::new("Accounts are invite-only. Ask the person who runs this Drop for a username. Closing this window keeps Drop in the tray.")
                .color(MUTED)
                .size(12.0),
        );
    }

    fn clipboard_ui(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        let account = self.account.clone();
        let Some(account) = account else {
            return;
        };
        ui.horizontal(|ui| {
            wordmark(ui);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add(Button::new("Quit")).clicked() {
                    self.request_quit(ctx);
                }
                if ui.add(Button::new("Sign out")).clicked() && !self.busy {
                    self.busy = true;
                    self.send(Command::SignOut);
                }
            });
        });
        ui.label(
            RichText::new(format!(
                "{} · {} of {} · kept {}",
                account.username,
                format_bytes(account.used_bytes),
                format_bytes(account.quota_bytes),
                retention_label(account.ttl_ms)
            ))
            .color(MUTED)
            .size(12.0),
        );
        if self.http {
            ui.label(
                RichText::new("This connection is not HTTPS. Items are still encrypted before they are uploaded.")
                    .color(MUTED)
                    .size(12.0),
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
            if ui.add_enabled(!self.busy, primary_button("Save text")).clicked() {
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
                ui.label(RichText::new("Nothing here yet. Drop a file on the tray icon, or save a note.").color(MUTED));
            }
            let items = self.items.clone();
            for item in items {
                self.item_card(ui, &item);
                ui.add_space(8.0);
            }
        });
        ui.add_space(6.0);
        let place = if cfg!(target_os = "macos") {
            "menu bar"
        } else {
            "notification area"
        };
        ui.label(
            RichText::new(format!("Closing this window keeps Drop in the {place}. Quit to forget the key."))
                .color(MUTED)
                .size(11.0),
        );
    }

    fn item_card(&mut self, ui: &mut egui::Ui, item: &ItemSummary) {
        Frame::none()
            .fill(CARD)
            .stroke(Stroke::new(1.0_f32, LINE))
            .inner_margin(Margin::same(10.0))
            .rounding(8.0)
            .show(ui, |ui| {
                ui.label(RichText::new(&item.title).strong().color(INK));
                ui.label(RichText::new(format!("{} · {}", item.detail, item.when)).color(MUTED).size(12.0));
                ui.horizontal(|ui| {
                    if item.can_copy && ui.add_enabled(!self.busy, Button::new("Copy")).clicked() {
                        self.busy = true;
                        self.error.clear();
                        self.send(Command::Copy(item.id.clone()));
                    }
                    if ui.add_enabled(!self.busy, Button::new("Download")).clicked() {
                        self.busy = true;
                        self.error.clear();
                        self.send(Command::Download(item.id.clone()));
                    }
                    let confirming = self.pending_delete.as_deref() == Some(item.id.as_str());
                    let label = if confirming { "Delete now" } else { "Delete" };
                    if ui.add_enabled(!self.busy, Button::new(label)).clicked() {
                        if confirming {
                            self.pending_delete = None;
                            self.busy = true;
                            self.send(Command::Delete(item.id.clone()));
                        } else {
                            self.pending_delete = Some(item.id.clone());
                        }
                    }
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
        if ctx.input(|input| input.viewport().close_requested()) && !self.quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
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
        let _ = self.tx.send(Command::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn show_window(ctx: &Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    #[cfg(target_os = "macos")]
    crate::mac_tray::activate();
}

fn apply_style(ctx: &Context) {
    let mut visuals = egui::Visuals::light();
    visuals.window_fill = CREAM;
    visuals.panel_fill = CREAM;
    visuals.extreme_bg_color = CARD;
    visuals.faint_bg_color = Color32::from_rgb(0xef, 0xe7, 0xd8);
    visuals.widgets.noninteractive.bg_fill = CARD;
    visuals.widgets.inactive.bg_fill = CARD;
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(0xe4, 0xf2, 0xe9);
    visuals.widgets.active.bg_fill = Color32::from_rgb(0xd7, 0xeb, 0xde);
    visuals.override_text_color = Some(INK);
    visuals.selection.bg_fill = Color32::from_rgb(0xcf, 0xe6, 0xd6);
    ctx.set_visuals(visuals);
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(10.0, 6.0);
    ctx.set_style(style);
}

fn wordmark(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(32.0), egui::Sense::hover());
        ui.painter().circle_filled(rect.center(), 16.0, GREEN);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "D",
            egui::FontId::proportional(18.0),
            ON_GREEN,
        );
        ui.vertical(|ui| {
            ui.label(RichText::new("Drop").size(22.0).strong().color(INK));
            ui.label(RichText::new("Private clipboard").size(11.0).color(MUTED));
        });
    });
}

fn labeled(ui: &mut egui::Ui, label: &str, value: &mut String, secret: bool) {
    ui.label(RichText::new(label).color(INK));
    let mut edit = TextEdit::singleline(value).desired_width(f32::INFINITY);
    if secret {
        edit = edit.password(true);
    }
    ui.add(edit);
}

fn primary_button(label: &str) -> Button<'_> {
    Button::new(RichText::new(label).color(ON_GREEN)).fill(GREEN).min_size(Vec2::new(0.0, 32.0))
}

fn notice(ui: &mut egui::Ui, error: &str, status: &str) {
    if !error.is_empty() {
        ui.add_space(6.0);
        ui.label(RichText::new(error).color(DANGER));
    } else if !status.is_empty() {
        ui.add_space(6.0);
        ui.label(RichText::new(status).color(MUTED));
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

use std::path::Path;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use drop_core::{CopyPayload, Downloaded, DropClient, DropError, Snapshot};
use zeroize::{Zeroize, Zeroizing};

pub enum Command {
    SignIn {
        server: String,
        username: String,
        password: Zeroizing<String>,
    },
    UploadText(String),
    UploadBytes {
        name: String,
        mime: String,
        bytes: Zeroizing<Vec<u8>>,
    },
    UploadPaths(Vec<std::path::PathBuf>),
    Copy(String),
    Download(String),
    Delete(String),
    SignOut,
    Shutdown,
}

pub enum WorkerEvent {
    Snapshot(Snapshot),
    Status(String),
    Error(String),
    SignedOut,
    Copy(CopyPayload),
    Download(Downloaded),
}

pub fn spawn(rx: Receiver<Command>, tx: Sender<WorkerEvent>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || run(rx, tx))
}

fn run(rx: Receiver<Command>, tx: Sender<WorkerEvent>) {
    let mut client: Option<DropClient> = None;
    loop {
        let command = match rx.recv_timeout(Duration::from_secs(4)) {
            Ok(command) => command,
            Err(RecvTimeoutError::Timeout) => {
                if needs_refresh(&client) {
                    if let Err(DropError::SignedOut) = refresh_client(&mut client, &tx) {
                        forget_session(&mut client, &tx);
                    }
                }
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => break,
        };

        match command {
            Command::Shutdown => {
                if let Some(mut client) = client.take() {
                    client.sign_out();
                }
                break;
            }
            Command::SignOut => {
                if let Some(mut client) = client.take() {
                    client.sign_out();
                }
                let _ = tx.send(WorkerEvent::SignedOut);
            }
            Command::SignIn {
                server,
                username,
                mut password,
            } => {
                if let Some(mut previous) = client.take() {
                    previous.sign_out();
                }
                let _ = tx.send(WorkerEvent::Status("Signing in…".into()));
                match DropClient::connect(&server) {
                    Ok(mut next) => match next.sign_in(&username, password.as_str()) {
                        Ok(snapshot) => {
                            password.zeroize();
                            client = Some(next);
                            let _ = tx.send(WorkerEvent::Status(String::new()));
                            let _ = tx.send(WorkerEvent::Snapshot(snapshot));
                        }
                        Err(error) => {
                            password.zeroize();
                            let _ = tx.send(WorkerEvent::Status(String::new()));
                            let _ = tx.send(WorkerEvent::Error(error.to_string()));
                        }
                    },
                    Err(error) => {
                        password.zeroize();
                        let _ = tx.send(WorkerEvent::Status(String::new()));
                        let _ = tx.send(WorkerEvent::Error(error.to_string()));
                    }
                }
            }
            Command::UploadText(text) => {
                let Some(client) = client.as_mut() else {
                    let _ = tx.send(WorkerEvent::Error(DropError::Locked.to_string()));
                    continue;
                };
                let _ = tx.send(WorkerEvent::Status("Encrypting text…".into()));
                report(client.upload_text(&text), &tx);
            }
            Command::UploadBytes { name, mime, mut bytes } => {
                let Some(client) = client.as_mut() else {
                    bytes.zeroize();
                    let _ = tx.send(WorkerEvent::Error(DropError::Locked.to_string()));
                    continue;
                };
                let _ = tx.send(WorkerEvent::Status(format!("Encrypting {name}…")));
                let result = client.upload_file(&name, &mime, &bytes);
                bytes.zeroize();
                report(result, &tx);
            }
            Command::UploadPaths(paths) => {
                let Some(client) = client.as_mut() else {
                    let _ = tx.send(WorkerEvent::Error("Sign in before dropping files.".into()));
                    continue;
                };
                let mut last = None;
                let mut failures = Vec::new();
                let mut uploaded = 0usize;
                for path in paths {
                    if path.is_dir() {
                        failures.push(DropError::Directory.to_string());
                        continue;
                    }
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "file".into());
                    let mime = mime_from_path(&path);
                    let bytes = match std::fs::read(&path) {
                        Ok(bytes) => Zeroizing::new(bytes),
                        Err(_) => {
                            failures.push(format!("Couldn't read {name}."));
                            continue;
                        }
                    };
                    let _ = tx.send(WorkerEvent::Status(format!("Encrypting {name}…")));
                    match client.upload_file(&name, &mime, &bytes) {
                        Ok(snapshot) => {
                            uploaded += 1;
                            last = Some(snapshot);
                        }
                        Err(error) => failures.push(error.to_string()),
                    }
                }
                if let Some(snapshot) = last {
                    let _ = tx.send(WorkerEvent::Snapshot(snapshot));
                }
                if failures.is_empty() {
                    let label = if uploaded == 1 { "Uploaded 1 file." } else { "" };
                    let message = if uploaded == 1 {
                        label.to_string()
                    } else {
                        format!("Uploaded {uploaded} files.")
                    };
                    let _ = tx.send(WorkerEvent::Status(message));
                } else {
                    let _ = tx.send(WorkerEvent::Error(failures.join(" ")));
                    if uploaded > 0 {
                        let _ = tx.send(WorkerEvent::Status(format!("Uploaded {uploaded}.")));
                    } else {
                        let _ = tx.send(WorkerEvent::Status(String::new()));
                    }
                }
            }
            Command::Copy(id) => {
                let Some(client) = client.as_mut() else {
                    let _ = tx.send(WorkerEvent::Error(DropError::Locked.to_string()));
                    continue;
                };
                match client.copy_item(&id) {
                    Ok(payload) => {
                        let _ = tx.send(WorkerEvent::Copy(payload));
                    }
                    Err(error) => {
                        let _ = tx.send(WorkerEvent::Error(error.to_string()));
                    }
                }
            }
            Command::Download(id) => {
                let Some(client) = client.as_mut() else {
                    let _ = tx.send(WorkerEvent::Error(DropError::Locked.to_string()));
                    continue;
                };
                match client.download_item(&id) {
                    Ok(file) => {
                        let _ = tx.send(WorkerEvent::Download(file));
                    }
                    Err(error) => {
                        let _ = tx.send(WorkerEvent::Error(error.to_string()));
                    }
                }
            }
            Command::Delete(id) => {
                let Some(client) = client.as_mut() else {
                    let _ = tx.send(WorkerEvent::Error(DropError::Locked.to_string()));
                    continue;
                };
                report(client.delete_item(&id), &tx);
            }
        }
    }
}

fn needs_refresh(client: &Option<DropClient>) -> bool {
    client.as_ref().is_some_and(|client| client.is_unlocked())
}

fn refresh_client(client: &mut Option<DropClient>, tx: &Sender<WorkerEvent>) -> Result<(), DropError> {
    let Some(current) = client.as_mut() else {
        return Ok(());
    };
    match current.refresh() {
        Ok(snapshot) => {
            let _ = tx.send(WorkerEvent::Snapshot(snapshot));
            Ok(())
        }
        Err(DropError::SignedOut) => Err(DropError::SignedOut),
        Err(error) => {
            let _ = tx.send(WorkerEvent::Error(error.to_string()));
            Ok(())
        }
    }
}

fn forget_session(client: &mut Option<DropClient>, tx: &Sender<WorkerEvent>) {
    if let Some(mut current) = client.take() {
        current.sign_out();
    }
    let _ = tx.send(WorkerEvent::SignedOut);
}

fn report(result: Result<drop_core::Snapshot, DropError>, tx: &Sender<WorkerEvent>) {
    match result {
        Ok(snapshot) => {
            let _ = tx.send(WorkerEvent::Status(String::new()));
            let _ = tx.send(WorkerEvent::Snapshot(snapshot));
        }
        Err(DropError::SignedOut) => {
            let _ = tx.send(WorkerEvent::SignedOut);
        }
        Err(error) => {
            let _ = tx.send(WorkerEvent::Status(String::new()));
            let _ = tx.send(WorkerEvent::Error(error.to_string()));
        }
    }
}

fn mime_from_path(path: &Path) -> String {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "txt" => "text/plain",
        "md" => "text/markdown",
        "json" => "application/json",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "html" | "htm" => "text/html",
        "csv" => "text/csv",
        "xml" => "application/xml",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "heic" => "image/heic",
        _ => "application/octet-stream",
    }
    .into()
}

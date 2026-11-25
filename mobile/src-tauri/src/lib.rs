//! Handy Mobile's core: remembers the paired desktop, keeps a link to it up,
//! and passes audio one way and transcripts the other.
//!
//! The link lives here, not in the WebView, because the desktop's
//! certificate is self-signed and pinned by fingerprint, which a WebView's
//! own WebSocket can't do.
//!
//! On Android the app is also a keyboard, which runs without the WebView and
//! keeps a link of its own: see `keyboard.rs`.

use handy_link::client::{self, Desktop, Link, LinkError, LinkSender};
use handy_link::pairing::PairingPayload;
use handy_link::protocol::{ClientMessage, ErrorCode, OutputMode, ServerMessage, SessionState};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;
use tauri::ipc::{InvokeBody, Request};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Notify;

#[cfg(target_os = "android")]
mod keyboard;

const DESKTOP_FILE: &str = "desktop.json";
/// Waits between reconnect attempts; the last one repeats.
const RETRY_DELAYS: [Duration; 4] = [
    Duration::from_millis(500),
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum LinkState {
    /// No desktop is paired.
    Unpaired,
    Connecting,
    Connected,
    /// The desktop can't be reached; trying again on its own.
    Offline,
    /// The desktop no longer knows this phone. Pairing again fixes it.
    Rejected,
    /// One of the two apps is too old for the other.
    Incompatible,
}

/// Everything the UI shows about the link, sent whole on every change.
#[derive(Clone, Debug, Serialize)]
struct Snapshot {
    link: LinkState,
    desktop_name: Option<String>,
    desktop_version: Option<String>,
    /// Where the desktop sends text unless this phone asks otherwise.
    default_output: Option<OutputMode>,
    session: SessionState,
    /// Why the last connection attempt failed, for the curious.
    detail: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct TranscriptEvent {
    text: String,
    pasted: bool,
}

#[derive(Clone, Debug, Serialize)]
struct ErrorEvent {
    code: ErrorCode,
    message: Option<String>,
    /// The transcript, when only the desktop's paste failed.
    text: Option<String>,
}

struct Inner {
    desktop: Option<Desktop>,
    sender: Option<LinkSender>,
    snapshot: Snapshot,
}

struct Core {
    inner: Mutex<Inner>,
    /// Bumped whenever the paired desktop changes, so a connection task for
    /// the old one stops instead of fighting the new one.
    generation: AtomicU64,
    /// Wakes a waiting reconnect: the user asked, or the app came back.
    retry_now: Notify,
}

type Shared = Arc<Core>;

fn lock(core: &Core) -> MutexGuard<'_, Inner> {
    core.inner.lock().unwrap_or_else(PoisonError::into_inner)
}

fn desktop_path(app: &AppHandle) -> Option<PathBuf> {
    // App-private storage: on Android no other app can read it. The keyboard
    // reads the same file (`HandyKeyboardService.kt` builds this path too).
    app.path()
        .app_data_dir()
        .ok()
        .map(|dir| dir.join(DESKTOP_FILE))
}

fn read_desktop(path: &Path) -> Option<Desktop> {
    let json = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&json).ok()
}

/// `tmp_extension` names the scratch file, so the app and the keyboard never
/// write the same one.
fn write_desktop(path: &Path, desktop: &Desktop, tmp_extension: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string(desktop).expect("a desktop serializes");
    // Renamed into place: a kill mid-write must not unpair.
    let tmp = path.with_extension(tmp_extension);
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)
}

fn load_desktop(app: &AppHandle) -> Option<Desktop> {
    read_desktop(&desktop_path(app)?)
}

fn save_desktop(app: &AppHandle, desktop: Option<&Desktop>) {
    let Some(path) = desktop_path(app) else {
        return;
    };
    let result = match desktop {
        Some(desktop) => write_desktop(&path, desktop, "json.tmp"),
        None => match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        },
    };
    if let Err(e) = result {
        log::error!("Failed to store the paired desktop: {e}");
    }
}

fn publish(app: &AppHandle, core: &Core, change: impl FnOnce(&mut Snapshot)) {
    let snapshot = {
        let mut inner = lock(core);
        change(&mut inner.snapshot);
        inner.snapshot.clone()
    };
    let _ = app.emit("link-snapshot", snapshot);
}

/// Keeps the link to the paired desktop up until the pairing changes.
fn spawn_connection(app: AppHandle, core: Shared) {
    let generation = core.generation.load(Ordering::Acquire);
    let current = move |core: &Core| core.generation.load(Ordering::Acquire) == generation;
    tauri::async_runtime::spawn(async move {
        let mut attempt = 0usize;
        loop {
            let Some(mut desktop) = lock(&core).desktop.clone() else {
                return;
            };
            if !current(&core) {
                return;
            }
            publish(&app, &core, |s| {
                // Stays "offline" while retrying, so the screen doesn't
                // flicker between two words for the same thing.
                if s.link != LinkState::Offline {
                    s.link = LinkState::Connecting;
                }
            });

            match client::connect(&mut desktop, "app").await {
                Ok((info, link)) => {
                    if !current(&core) {
                        return;
                    }
                    attempt = 0;
                    // It may have been found at a new address, under a new name.
                    save_desktop(&app, Some(&desktop));
                    {
                        let mut inner = lock(&core);
                        inner.desktop = Some(desktop);
                        inner.sender = Some(link.sender());
                    }
                    publish(&app, &core, |s| {
                        s.link = LinkState::Connected;
                        s.desktop_name = Some(info.name);
                        s.desktop_version = Some(info.version);
                        s.default_output = Some(info.default_output);
                        s.session = SessionState::Idle;
                        s.detail = None;
                    });
                    pump(&app, &core, link, &current).await;
                    if !current(&core) {
                        return;
                    }
                    lock(&core).sender = None;
                    publish(&app, &core, |s| {
                        s.link = LinkState::Offline;
                        s.session = SessionState::Idle;
                    });
                }
                Err(LinkError::Refused(ErrorCode::Unauthorized)) => {
                    publish(&app, &core, |s| s.link = LinkState::Rejected);
                    return;
                }
                Err(LinkError::Refused(ErrorCode::UnsupportedProtocol)) => {
                    publish(&app, &core, |s| s.link = LinkState::Incompatible);
                    return;
                }
                Err(e) => {
                    if !current(&core) {
                        return;
                    }
                    log::info!("Couldn't reach the desktop: {e}");
                    publish(&app, &core, |s| {
                        s.link = LinkState::Offline;
                        s.detail = Some(e.to_string());
                    });
                }
            }

            let delay = RETRY_DELAYS[attempt.min(RETRY_DELAYS.len() - 1)];
            attempt += 1;
            tokio::select! {
                _ = tokio::time::sleep(delay) => {}
                _ = core.retry_now.notified() => attempt = 0,
            }
        }
    });
}

/// Passes on what the desktop says until the link drops.
async fn pump(app: &AppHandle, core: &Shared, mut link: Link, current: &impl Fn(&Core) -> bool) {
    while let Some(message) = link.recv().await {
        if !current(core) {
            return;
        }
        match message {
            ServerMessage::State { state } => publish(app, core, |s| s.session = state),
            ServerMessage::Final { text, pasted, .. } => {
                let _ = app.emit("transcript", TranscriptEvent { text, pasted });
            }
            ServerMessage::Error {
                code,
                message,
                text,
            } => {
                let _ = app.emit(
                    "link-error",
                    ErrorEvent {
                        code,
                        message,
                        text,
                    },
                );
            }
            ServerMessage::Paired { .. }
            | ServerMessage::Ready { .. }
            | ServerMessage::ModelChanged { .. }
            | ServerMessage::Pong => {}
        }
    }
}

fn send(core: &Core, message: ClientMessage) -> Result<(), String> {
    let sender = lock(core).sender.clone().ok_or("not_connected")?;
    sender
        .send(message)
        .map_err(|_| "not_connected".to_string())
}

#[tauri::command]
fn get_snapshot(core: State<'_, Shared>) -> Snapshot {
    lock(&core).snapshot.clone()
}

/// Pairs with the desktop a scanned or pasted code describes. Errors are
/// codes the UI has words for.
#[tauri::command]
async fn pair(
    app: AppHandle,
    core: State<'_, Shared>,
    code: String,
    device_name: String,
) -> Result<(), String> {
    let payload = PairingPayload::decode(&code).map_err(|e| {
        use handy_link::pairing::PairingPayloadError::*;
        match e {
            NotAPairingCode | Incomplete => "invalid_code",
            UnsupportedVersion => "incompatible",
        }
        .to_string()
    })?;
    let me = client::ClientInfo {
        device_kind: Some(handy_link::protocol::device_kind::PHONE.to_string()),
        features: Vec::new(),
    };
    let (desktop, info, link) = client::pair_with(&payload, &device_name, &me)
        .await
        .map_err(|e| {
            log::warn!("Pairing failed: {e}");
            match e {
                LinkError::Refused(ErrorCode::InvalidToken) => "expired_code",
                LinkError::Refused(ErrorCode::UnsupportedProtocol) => "incompatible",
                LinkError::Refused(ErrorCode::RateLimited) => "rate_limited",
                LinkError::Unreachable(_) => "unreachable",
                LinkError::Handshake(_) => "untrusted",
                LinkError::Refused(_) | LinkError::Closed => "failed",
            }
            .to_string()
        })?;
    // The pairing connection has done its job; the connection task makes
    // its own, the same way every later launch will.
    drop(link);

    save_desktop(&app, Some(&desktop));
    core.generation.fetch_add(1, Ordering::AcqRel);
    {
        let mut inner = lock(&core);
        inner.desktop = Some(desktop);
        inner.sender = None;
    }
    publish(&app, &core, |s| {
        s.link = LinkState::Connecting;
        s.desktop_name = Some(info.name);
        s.desktop_version = Some(info.version);
        s.default_output = Some(info.default_output);
        s.session = SessionState::Idle;
        s.detail = None;
    });
    spawn_connection(app.clone(), Arc::clone(&core));
    Ok(())
}

/// Forgets the desktop. It still lists this phone until unpaired there too,
/// but nothing here can use that pairing any more.
#[tauri::command]
fn unpair(app: AppHandle, core: State<'_, Shared>) {
    core.generation.fetch_add(1, Ordering::AcqRel);
    save_desktop(&app, None);
    {
        let mut inner = lock(&core);
        inner.desktop = None;
        // Dropping the last sender closes the link.
        inner.sender = None;
    }
    core.retry_now.notify_waiters();
    publish(&app, &core, |s| {
        *s = Snapshot {
            link: LinkState::Unpaired,
            desktop_name: None,
            desktop_version: None,
            default_output: None,
            session: SessionState::Idle,
            detail: None,
        }
    });
}

/// Tries to reach the desktop now instead of at the next retry.
#[tauri::command]
fn reconnect(core: State<'_, Shared>) {
    core.retry_now.notify_waiters();
}

#[tauri::command]
fn start_recording(
    core: State<'_, Shared>,
    output: Option<OutputMode>,
    post_process: Option<bool>,
) -> Result<(), String> {
    send(
        &core,
        ClientMessage::Start {
            output,
            post_process,
            options: None,
        },
    )
}

#[tauri::command]
fn stop_recording(core: State<'_, Shared>) -> Result<(), String> {
    send(&core, ClientMessage::Stop)
}

#[tauri::command]
fn cancel_recording(core: State<'_, Shared>) -> Result<(), String> {
    send(&core, ClientMessage::Cancel)
}

/// One frame of 16 kHz mono 16-bit PCM, as the raw body of the call.
#[tauri::command]
fn send_audio(core: State<'_, Shared>, request: Request<'_>) -> Result<(), String> {
    let pcm = match request.body() {
        InvokeBody::Raw(bytes) => bytes.clone(),
        // Where the WebView can't send bytes, they arrive as a JSON array.
        InvokeBody::Json(value) => {
            serde_json::from_value::<Vec<u8>>(value.clone()).map_err(|_| "bad_audio".to_string())?
        }
    };
    let sender = lock(&core).sender.clone().ok_or("not_connected")?;
    sender
        .send_audio(pcm)
        .map_err(|_| "not_connected".to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    // The scanner only exists on phones; on a desktop (handy for working on
    // the UI) the code is typed in instead.
    #[cfg(mobile)]
    let builder = builder.plugin(tauri_plugin_barcode_scanner::init());
    builder
        .setup(|app| {
            let handle = app.handle().clone();
            let desktop = load_desktop(&handle);
            let core: Shared = Arc::new(Core {
                inner: Mutex::new(Inner {
                    snapshot: Snapshot {
                        link: if desktop.is_some() {
                            LinkState::Connecting
                        } else {
                            LinkState::Unpaired
                        },
                        desktop_name: desktop.as_ref().map(|d| d.desktop_name.clone()),
                        desktop_version: None,
                        default_output: None,
                        session: SessionState::Idle,
                        detail: None,
                    },
                    desktop,
                    sender: None,
                }),
                generation: AtomicU64::new(0),
                retry_now: Notify::new(),
            });
            app.manage(Arc::clone(&core));
            spawn_connection(handle, core);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            pair,
            unpair,
            reconnect,
            start_recording,
            stop_recording,
            cancel_recording,
            send_audio,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Handy Mobile");
}

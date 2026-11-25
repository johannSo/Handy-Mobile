//! Transcribing with another computer: this computer is a client of a Handy
//! that shares its model (see `mobile.rs` for that side), the way a phone is.
//!
//! Only the speech-to-text happens over there. The recording, history,
//! post-processing, the user's hook and the paste all stay here, so this
//! computer's settings and API keys apply and its text never leaves it
//! except as audio. The other computer gets this one's text settings
//! (language, custom words, filler words, Chinese script) with each request,
//! so the transcript comes back finished the way a local one would be.
//!
//! While the engine is set to another computer, a link to it is kept up so
//! the status is truthful and a dictation doesn't wait for a handshake.
//! Requests go one at a time over it; dictation here and file jobs queue.
//!
//! The paired computers live in their own file, not in the settings store:
//! the frontend gets every setting, and the secrets have no business there.

use anyhow::{anyhow, Result};
use handy_link::client::{self, ClientInfo, ConnectOptions, Desktop, LinkError, LinkSender};
use handy_link::pairing::PairingPayload;
use handy_link::protocol::{
    device_kind, f32_to_pcm16, feature, ClientMessage, ErrorCode, ModelInfo, OutputMode,
    ServerMessage, SessionState, StartOptions, TextOptions,
};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use tauri_specta::Event;
use tokio::sync::{mpsc, watch, Notify};

use crate::language_detection::{LanguageDecision, LanguageSource};
use crate::managers::transcription::{ChunkControl, Transcription, TranscriptionManager};
use crate::notices::NoticeCode;
use crate::settings::{get_settings, write_settings, AppSettings, TranscriptionEngine};

const STORE_FILE: &str = "remote_servers.json";
/// The channel this app's connection uses on the other computer.
const CHANNEL: &str = "desktop";
/// Waits between reconnect attempts; the last one repeats. Slower than a
/// phone's: a laptop away from home shouldn't keep knocking.
const RETRY_DELAYS: [Duration; 6] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(10),
    Duration::from_secs(30),
    Duration::from_secs(60),
];
/// How long a request waits for the link to come up.
const CONNECT_WAIT: Duration = Duration::from_secs(8);
/// How long a request waits for the other computer to answer a message.
const ANSWER_WAIT: Duration = Duration::from_secs(10);
/// How long a request waits for the other computer to be free: its own
/// user's dictation, a phone, or the end of a request this one cancelled.
const BUSY_WAIT: Duration = Duration::from_secs(30);
const BUSY_POLL: Duration = Duration::from_millis(400);
/// How long the other computer may take on top of the audio's length.
const TRANSCRIBE_SLACK: Duration = Duration::from_secs(120);
const CANCEL_POLL: Duration = Duration::from_millis(25);
/// Audio is sent in frames of this many samples (half a second), well
/// under the other computer's message limit.
const FRAME_SAMPLES: usize = 8_000;
/// The longest recording sent in one request, safely inside the other
/// computer's limit of 30 minutes. A longer one goes in parts.
const MAX_REQUEST_SAMPLES: usize = 20 * 60 * 16_000;
/// Audio handed to the chunker at a time in a file job, as a local job does.
const FILE_FEED_SAMPLES: usize = 60 * 16_000;

#[derive(Clone, Serialize, Deserialize)]
struct StoredServer {
    #[serde(flatten)]
    desktop: Desktop,
    /// What the user calls it, over the name the computer gives itself.
    #[serde(default)]
    label: Option<String>,
    paired_at: i64,
}

#[derive(Default, Serialize, Deserialize)]
struct ServerStore {
    #[serde(default)]
    servers: Vec<StoredServer>,
}

/// Where the link to the selected computer stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RemoteLinkState {
    /// Not transcribing with another computer, or none is selected.
    Unused,
    Connecting,
    Connected,
    /// It can't be reached; trying again on its own.
    Offline,
    /// It no longer knows this computer. Pairing again fixes it.
    Rejected,
    /// One of the two is too old for the other.
    Incompatible,
}

/// A paired computer, as the Models tab lists it.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct RemoteServer {
    pub id: String,
    pub name: String,
    /// What it calls itself.
    pub host_name: String,
    pub addresses: Vec<String>,
    pub port: u16,
    /// The start of its certificate's fingerprint, to tell two apart.
    pub fingerprint: String,
    /// Unix seconds.
    pub paired_at: i64,
}

/// The model a computer transcribes with.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct RemoteModel {
    pub id: String,
    pub name: String,
    pub languages: Vec<String>,
    pub supports_language_selection: bool,
    pub supports_translation: bool,
    pub supports_language_detection: bool,
}

impl From<ModelInfo> for RemoteModel {
    fn from(model: ModelInfo) -> Self {
        Self {
            id: model.id,
            name: model.name,
            languages: model.languages,
            supports_language_selection: model.supports_language_selection,
            supports_translation: model.supports_translation,
            supports_language_detection: model.supports_language_detection,
        }
    }
}

/// Everything the UI shows about transcribing with another computer.
#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct RemoteStatus {
    /// The engine is set to another computer and one is selected.
    pub in_use: bool,
    pub selected: Option<String>,
    pub link: RemoteLinkState,
    /// Its version, once connected.
    pub version: Option<String>,
    /// The model it transcribes with, once connected. `None` there means it
    /// has none.
    pub model: Option<RemoteModel>,
    /// A request is under way.
    pub busy: bool,
    /// Why the last attempt failed, for the curious.
    pub detail: Option<String>,
    pub servers: Vec<RemoteServer>,
    /// Debug: requests fail as if the computer were unreachable.
    pub simulate_offline: bool,
}

/// Why a request didn't produce text. The dictation and file paths turn
/// these into notices.
#[derive(Debug)]
pub enum RemoteError {
    Unreachable(String),
    Busy,
    NoModel,
    /// It doesn't know this computer any more.
    Rejected,
    Incompatible,
    Failed(String),
    Cancelled,
}

impl std::fmt::Display for RemoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreachable(why) => write!(f, "the other computer can't be reached: {why}"),
            Self::Busy => f.write_str("the other computer is busy"),
            Self::NoModel => f.write_str("the other computer has no model"),
            Self::Rejected => f.write_str("the other computer no longer knows this one"),
            Self::Incompatible => f.write_str("the other computer's Handy is too old or too new"),
            Self::Failed(why) => write!(f, "the other computer couldn't transcribe: {why}"),
            Self::Cancelled => f.write_str("cancelled"),
        }
    }
}

impl std::error::Error for RemoteError {}

/// How a pairing failed, as codes the UI has words for.
#[derive(Debug)]
pub struct PairError(pub &'static str);

struct Inner {
    /// `None` until first needed.
    store: Option<ServerStore>,
    sender: Option<LinkSender>,
    /// Who hears what the other computer says next: the request under way.
    listener: Option<mpsc::UnboundedSender<ServerMessage>>,
    status: RemoteStatus,
    /// Whether the connected computer answers `prepare`.
    can_prepare: bool,
}

pub struct RemoteEngineManager {
    app: AppHandle,
    inner: Mutex<Inner>,
    /// Bumped whenever the computer to connect to changes, so a connection
    /// task for the old one stops instead of fighting the new one.
    generation: AtomicU64,
    /// Wakes a waiting reconnect: a request needs the link now.
    retry_now: Notify,
    /// The link's state, for a request waiting for it to come up.
    link_state: watch::Sender<RemoteLinkState>,
    /// One request at a time over the link.
    turn: tokio::sync::Mutex<()>,
    simulate_offline: AtomicBool,
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn server_view(server: &StoredServer) -> RemoteServer {
    RemoteServer {
        id: server.desktop.desktop_id.clone(),
        name: server
            .label
            .clone()
            .unwrap_or_else(|| server.desktop.desktop_name.clone()),
        host_name: server.desktop.desktop_name.clone(),
        addresses: server
            .desktop
            .addresses
            .iter()
            .map(ToString::to_string)
            .collect(),
        port: server.desktop.port,
        fingerprint: server.desktop.fingerprint.chars().take(16).collect(),
        paired_at: server.paired_at,
    }
}

/// A setting that is an enum, as its JSON spells it.
fn spelled<T: Serialize>(value: &T) -> Option<String> {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
}

/// This computer's text settings, for the other computer to apply.
fn text_options(settings: &AppSettings) -> TextOptions {
    TextOptions {
        selected_language: Some(settings.selected_language.clone()),
        spoken_languages: Some(settings.spoken_languages.clone()),
        translate_to_english: Some(settings.translate_to_english),
        language_detection: spelled(&settings.language_detection),
        custom_words: Some(settings.custom_words.clone()),
        word_correction_threshold: Some(settings.word_correction_threshold),
        filler_word_removal: Some(settings.filler_word_removal_enabled),
        custom_filler_words: settings.custom_filler_words.clone(),
        chinese_script: spelled(&settings.chinese_script),
    }
}

fn language_decision(language: Option<String>, source: Option<String>) -> Option<LanguageDecision> {
    let source = match source.as_deref()? {
        "selected" => LanguageSource::Selected,
        "model" => LanguageSource::Model,
        "detected" => LanguageSource::Detected,
        "fallback" => LanguageSource::Fallback,
        _ => return None,
    };
    Some(LanguageDecision { language, source })
}

/// Waits for `future`, giving up when `cancelled` says so.
async fn unless_cancelled<F: std::future::Future, C: Fn() -> bool + ?Sized>(
    future: F,
    cancelled: &C,
) -> Result<F::Output, RemoteError> {
    tokio::pin!(future);
    loop {
        if cancelled() {
            return Err(RemoteError::Cancelled);
        }
        if let Ok(output) = tokio::time::timeout(CANCEL_POLL, future.as_mut()).await {
            return Ok(output);
        }
    }
}

/// Sends `Cancel` if a request is dropped before its answer came, so the
/// other computer stops working on it.
struct CancelOnDrop {
    sender: Option<LinkSender>,
}

impl CancelOnDrop {
    fn disarm(&mut self) {
        self.sender = None;
    }
}

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(ClientMessage::Cancel);
        }
    }
}

impl RemoteEngineManager {
    pub fn new(app: &AppHandle) -> Arc<Self> {
        let (link_state, _) = watch::channel(RemoteLinkState::Unused);
        Arc::new(Self {
            app: app.clone(),
            inner: Mutex::new(Inner {
                store: None,
                sender: None,
                listener: None,
                status: RemoteStatus {
                    in_use: false,
                    selected: None,
                    link: RemoteLinkState::Unused,
                    version: None,
                    model: None,
                    busy: false,
                    detail: None,
                    servers: Vec::new(),
                    simulate_offline: false,
                },
                can_prepare: false,
            }),
            generation: AtomicU64::new(0),
            retry_now: Notify::new(),
            link_state,
            turn: tokio::sync::Mutex::new(()),
            simulate_offline: AtomicBool::new(false),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn store_path(&self) -> Result<PathBuf> {
        Ok(crate::portable::app_data_dir(&self.app)?.join(STORE_FILE))
    }

    fn store<'a>(&self, inner: &'a mut Inner) -> &'a mut ServerStore {
        if inner.store.is_none() {
            let loaded = self
                .store_path()
                .ok()
                .and_then(|path| std::fs::read_to_string(path).ok())
                .and_then(|json| match serde_json::from_str::<ServerStore>(&json) {
                    Ok(store) => Some(store),
                    Err(e) => {
                        warn!("{STORE_FILE} is unreadable ({e}); computers must pair again");
                        None
                    }
                });
            inner.store = Some(loaded.unwrap_or_default());
        }
        inner.store.as_mut().expect("just set")
    }

    fn save(&self, store: &ServerStore) -> Result<()> {
        let path = self.store_path()?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // Renamed into place: a crash mid-write must not unpair everything.
        let tmp = path.with_extension("json.tmp");
        {
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create(true).truncate(true);
            // The secrets are in here: nobody else on this computer reads it.
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&tmp)?;
            std::io::Write::write_all(&mut file, serde_json::to_string_pretty(store)?.as_bytes())?;
            file.sync_all()?;
        }
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// The selected computer, if the engine is set to another computer and
    /// it is still paired.
    fn selected(&self, settings: &AppSettings) -> Option<StoredServer> {
        if settings.transcription_engine != TranscriptionEngine::Remote {
            return None;
        }
        let id = settings.remote_server_id.as_deref()?;
        let mut inner = self.lock();
        self.store(&mut inner)
            .servers
            .iter()
            .find(|server| server.desktop.desktop_id == id)
            .cloned()
    }

    /// Whether dictation and file jobs go to another computer.
    pub fn in_use(&self) -> bool {
        self.selected(&get_settings(&self.app)).is_some()
    }

    /// Changes the status and tells the UI and the tray.
    fn publish(&self, change: impl FnOnce(&mut RemoteStatus)) {
        let status = {
            let mut inner = self.lock();
            change(&mut inner.status);
            let servers: Vec<RemoteServer> = self
                .store(&mut inner)
                .servers
                .iter()
                .map(server_view)
                .collect();
            inner.status.servers = servers;
            inner.status.simulate_offline = self.simulate_offline.load(Ordering::Acquire);
            inner.status.clone()
        };
        self.link_state.send_replace(status.link);
        if let Err(e) = status.emit(&self.app) {
            warn!("Failed to emit the remote status: {e}");
        }
        crate::tray::update_tray_menu(&self.app);
    }

    pub fn status(&self) -> RemoteStatus {
        let mut inner = self.lock();
        let servers: Vec<RemoteServer> = self
            .store(&mut inner)
            .servers
            .iter()
            .map(server_view)
            .collect();
        inner.status.servers = servers;
        inner.status.simulate_offline = self.simulate_offline.load(Ordering::Acquire);
        inner.status.clone()
    }

    /// Connects to the selected computer, or lets go of it, to match the
    /// settings. Call at startup and whenever the engine or the selected
    /// computer change.
    pub fn apply_settings(self: &Arc<Self>) {
        let settings = get_settings(&self.app);
        let selected = self.selected(&settings);
        let selected_id = selected.as_ref().map(|s| s.desktop.desktop_id.clone());
        {
            let inner = self.lock();
            let unchanged = inner.status.selected == selected_id
                && inner.status.in_use == selected.is_some()
                && !matches!(
                    inner.status.link,
                    RemoteLinkState::Rejected | RemoteLinkState::Incompatible
                );
            if unchanged {
                return;
            }
        }
        self.generation.fetch_add(1, Ordering::AcqRel);
        {
            let mut inner = self.lock();
            // Dropping the last sender closes the old link.
            inner.sender = None;
            inner.listener = None;
            inner.can_prepare = false;
        }
        let in_use = selected.is_some();
        self.publish(|s| {
            s.in_use = in_use;
            s.selected = selected_id.clone();
            s.link = if in_use {
                RemoteLinkState::Connecting
            } else {
                RemoteLinkState::Unused
            };
            s.version = None;
            s.model = None;
            s.busy = false;
            s.detail = None;
        });
        if in_use {
            self.spawn_connection();
        }
    }

    /// Keeps the link to the selected computer up until the selection
    /// changes.
    fn spawn_connection(self: &Arc<Self>) {
        let generation = self.generation.load(Ordering::Acquire);
        let manager = Arc::clone(self);
        let current = move |m: &Self| m.generation.load(Ordering::Acquire) == generation;
        tauri::async_runtime::spawn(async move {
            let mut attempt = 0usize;
            // Look on the network on the first try and whenever asked to,
            // not on every retry.
            let mut discover = true;
            loop {
                if !current(&manager) {
                    return;
                }
                let Some(mut server) = manager.selected(&get_settings(&manager.app)) else {
                    return;
                };
                manager.publish(|s| {
                    // Stays "offline" while retrying, so the UI doesn't
                    // flicker between two words for the same thing.
                    if s.link != RemoteLinkState::Offline {
                        s.link = RemoteLinkState::Connecting;
                    }
                });
                let options = ConnectOptions {
                    client: client_info(),
                    discover,
                };
                discover = false;
                match client::connect_with(&mut server.desktop, CHANNEL, &options).await {
                    Ok((info, mut link)) => {
                        if !current(&manager) {
                            return;
                        }
                        if !info.has(feature::RAW_TEXT) {
                            warn!(
                                "{} runs a Handy that can't transcribe for another computer",
                                info.name
                            );
                            manager.publish(|s| {
                                s.link = RemoteLinkState::Incompatible;
                                s.version = Some(info.version.clone());
                            });
                            return;
                        }
                        attempt = 0;
                        // It may have been found at a new address, under a
                        // new name.
                        manager.remember(&server.desktop);
                        {
                            let mut inner = manager.lock();
                            inner.sender = Some(link.sender());
                            inner.can_prepare = info.has(feature::PREPARE);
                        }
                        info!("Connected to {} to transcribe with it", info.name);
                        manager.publish(|s| {
                            s.link = RemoteLinkState::Connected;
                            s.version = Some(info.version.clone());
                            s.model = info.model.clone().map(RemoteModel::from);
                            s.detail = None;
                        });
                        while let Some(message) = link.recv().await {
                            if !current(&manager) {
                                return;
                            }
                            match message {
                                ServerMessage::ModelChanged { model } => {
                                    manager.publish(|s| s.model = model.map(RemoteModel::from));
                                }
                                message => {
                                    let listener = manager.lock().listener.clone();
                                    if let Some(listener) = listener {
                                        let _ = listener.send(message);
                                    }
                                }
                            }
                        }
                        if !current(&manager) {
                            return;
                        }
                        {
                            let mut inner = manager.lock();
                            inner.sender = None;
                            // The request under way hears that the link went.
                            inner.listener = None;
                        }
                        manager.publish(|s| s.link = RemoteLinkState::Offline);
                        // Soon back: it was there a moment ago.
                        discover = true;
                    }
                    Err(LinkError::Refused(ErrorCode::Unauthorized)) => {
                        manager.publish(|s| s.link = RemoteLinkState::Rejected);
                        return;
                    }
                    Err(LinkError::Refused(ErrorCode::UnsupportedProtocol)) => {
                        manager.publish(|s| s.link = RemoteLinkState::Incompatible);
                        return;
                    }
                    Err(e) => {
                        if !current(&manager) {
                            return;
                        }
                        debug!("Couldn't reach the computer to transcribe with: {e}");
                        manager.publish(|s| {
                            s.link = RemoteLinkState::Offline;
                            s.detail = Some(e.to_string());
                        });
                    }
                }
                let delay = RETRY_DELAYS[attempt.min(RETRY_DELAYS.len() - 1)];
                attempt += 1;
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {}
                    _ = manager.retry_now.notified() => {
                        attempt = 0;
                        discover = true;
                    }
                }
            }
        });
    }

    /// Saves where a computer was found and what it calls itself now.
    fn remember(&self, desktop: &Desktop) {
        let mut inner = self.lock();
        let store = self.store(&mut inner);
        let Some(stored) = store
            .servers
            .iter_mut()
            .find(|s| s.desktop.desktop_id == desktop.desktop_id)
        else {
            return;
        };
        if stored.desktop == *desktop {
            return;
        }
        stored.desktop = desktop.clone();
        if let Err(e) = self.save(store) {
            warn!("Failed to save where the other computer was found: {e}");
        }
    }

    /// Tries to reach the selected computer now instead of at the next
    /// retry, and starts over after a refusal.
    pub fn reconnect(self: &Arc<Self>) {
        let refused = matches!(
            self.lock().status.link,
            RemoteLinkState::Rejected | RemoteLinkState::Incompatible
        );
        if refused {
            // Its connection task has stopped; start a new one.
            self.lock().status.selected = None;
            self.apply_settings();
        } else {
            self.retry_now.notify_one();
        }
    }

    /// A dictation is starting: wake the link if it's down, and have the
    /// other computer load its model while the user talks.
    pub fn prepare(&self) {
        let (sender, can_prepare, link) = {
            let inner = self.lock();
            (inner.sender.clone(), inner.can_prepare, inner.status.link)
        };
        match sender {
            Some(sender) if can_prepare => {
                let _ = sender.send(ClientMessage::Prepare);
            }
            Some(_) => {}
            None if link == RemoteLinkState::Offline || link == RemoteLinkState::Connecting => {
                self.retry_now.notify_one();
            }
            None => {}
        }
    }

    /// Pairs with the computer a pasted code describes, and transcribes
    /// with it from now on.
    pub async fn pair(self: &Arc<Self>, code: &str) -> Result<RemoteServer, PairError> {
        let payload = PairingPayload::decode(code).map_err(|e| {
            use handy_link::pairing::PairingPayloadError::*;
            PairError(match e {
                NotAPairingCode | Incomplete => "invalid_code",
                UnsupportedVersion => "incompatible",
            })
        })?;
        let own_id = self
            .app
            .state::<Arc<crate::managers::mobile::MobileLinkManager>>()
            .desktop_id();
        if own_id.as_deref() == Some(payload.desktop_id.as_str()) {
            return Err(PairError("self"));
        }
        let name = crate::managers::mobile::desktop_name();
        let (desktop, info, link) = client::pair_with(&payload, &name, &client_info())
            .await
            .map_err(|e| {
                warn!("Pairing with another computer failed: {e}");
                PairError(match e {
                    LinkError::Refused(ErrorCode::InvalidToken) => "expired_code",
                    LinkError::Refused(ErrorCode::UnsupportedProtocol) => "incompatible",
                    LinkError::Refused(ErrorCode::RateLimited) => "rate_limited",
                    LinkError::Unreachable(_) => "unreachable",
                    LinkError::Handshake(_) => "untrusted",
                    LinkError::Refused(_) | LinkError::Closed => "failed",
                })
            })?;
        // The connection task makes its own, as every later start will.
        drop(link);
        if !info.has(feature::RAW_TEXT) {
            return Err(PairError("incompatible"));
        }
        let server = StoredServer {
            desktop,
            label: None,
            paired_at: now(),
        };
        let view = server_view(&server);
        {
            let mut inner = self.lock();
            let store = self.store(&mut inner);
            // Pairing again with the same computer replaces the old pairing.
            store
                .servers
                .retain(|s| s.desktop.desktop_id != server.desktop.desktop_id);
            store.servers.push(server);
            self.save(store).map_err(|e| {
                warn!("Failed to save a paired computer: {e}");
                PairError("failed")
            })?;
        }
        info!("Paired with {} to transcribe with it", info.name);
        let mut settings = get_settings(&self.app);
        settings.remote_server_id = Some(view.id.clone());
        settings.transcription_engine = TranscriptionEngine::Remote;
        write_settings(&self.app, settings);
        self.settings_changed();
        // A new pairing for the selected computer needs a new link too.
        self.lock().status.selected = None;
        self.apply_settings();
        Ok(view)
    }

    /// Forgets a computer. It still lists this one until unpaired there too,
    /// but nothing here can use that pairing any more.
    pub fn forget(self: &Arc<Self>, id: &str) -> Result<()> {
        let remaining: Vec<String> = {
            let mut inner = self.lock();
            let store = self.store(&mut inner);
            store.servers.retain(|s| s.desktop.desktop_id != id);
            self.save(store)?;
            store
                .servers
                .iter()
                .map(|s| s.desktop.desktop_id.clone())
                .collect()
        };
        let mut settings = get_settings(&self.app);
        if settings.remote_server_id.as_deref() == Some(id) {
            // Another paired computer takes its place, or this one's model.
            settings.remote_server_id = remaining.first().cloned();
            if settings.remote_server_id.is_none() {
                settings.transcription_engine = TranscriptionEngine::Local;
            }
            write_settings(&self.app, settings);
            self.settings_changed();
        }
        self.apply_settings();
        self.publish(|_| {});
        Ok(())
    }

    /// The settings window re-reads the settings this changed.
    fn settings_changed(&self) {
        use tauri::Emitter;
        let _ = self.app.emit(
            "settings-changed",
            serde_json::json!({ "setting": "transcription_engine" }),
        );
    }

    pub fn rename(&self, id: &str, name: &str) -> Result<()> {
        let name: String = name.chars().filter(|c| !c.is_control()).take(40).collect();
        let name = name.trim();
        {
            let mut inner = self.lock();
            let store = self.store(&mut inner);
            let server = store
                .servers
                .iter_mut()
                .find(|s| s.desktop.desktop_id == id)
                .ok_or_else(|| anyhow!("That computer isn't paired"))?;
            server.label = (!name.is_empty()).then(|| name.to_string());
            self.save(store)?;
        }
        self.publish(|_| {});
        Ok(())
    }

    pub fn set_simulate_offline(&self, enabled: bool) {
        self.simulate_offline.store(enabled, Ordering::Release);
        self.publish(|_| {});
    }

    /// Waits for the link, waking it if it's down. The sender to use.
    async fn connected<C: Fn() -> bool + ?Sized>(
        &self,
        cancelled: &C,
    ) -> Result<LinkSender, RemoteError> {
        let deadline = Instant::now() + CONNECT_WAIT;
        let mut state = self.link_state.subscribe();
        let mut woke = false;
        loop {
            {
                let inner = self.lock();
                if let Some(sender) = &inner.sender {
                    return Ok(sender.clone());
                }
                match inner.status.link {
                    RemoteLinkState::Rejected => return Err(RemoteError::Rejected),
                    RemoteLinkState::Incompatible => return Err(RemoteError::Incompatible),
                    RemoteLinkState::Unused => {
                        return Err(RemoteError::Unreachable("no computer is selected".into()))
                    }
                    _ => {}
                }
            }
            if !woke {
                woke = true;
                self.retry_now.notify_one();
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                let detail = self.lock().status.detail.clone();
                return Err(RemoteError::Unreachable(
                    detail.unwrap_or_else(|| "it didn't answer in time".into()),
                ));
            }
            let _ =
                unless_cancelled(tokio::time::timeout(left, state.changed()), cancelled).await?;
        }
    }

    /// Transcribes `samples` with the selected computer. A recording longer
    /// than one request allows goes in parts, joined as a dictation's
    /// chunks are.
    pub async fn transcribe<C: Fn() -> bool + ?Sized>(
        &self,
        samples: &[f32],
        cancelled: &C,
    ) -> Result<Transcription, RemoteError> {
        if self.simulate_offline.load(Ordering::Acquire) {
            return Err(RemoteError::Unreachable("simulated (Debug)".into()));
        }
        if samples.is_empty() {
            return Ok(Transcription {
                text: String::new(),
                language: None,
            });
        }
        let _turn = unless_cancelled(self.turn.lock(), cancelled).await?;
        self.publish(|s| s.busy = true);
        struct NotBusy<'a>(&'a RemoteEngineManager);
        impl Drop for NotBusy<'_> {
            fn drop(&mut self) {
                self.0.lock().listener = None;
                self.0.publish(|s| s.busy = false);
            }
        }
        let _not_busy = NotBusy(self);

        let options = StartOptions {
            raw: true,
            text: Some(text_options(&get_settings(&self.app))),
        };
        let mut parts = Vec::new();
        let mut language = None;
        for part in samples.chunks(MAX_REQUEST_SAMPLES) {
            let transcription = self.request(part, &options, cancelled).await?;
            if language.is_none() {
                language = transcription.language;
            }
            let text = transcription.text.trim().to_string();
            if !text.is_empty() {
                parts.push(text);
            }
        }
        Ok(Transcription {
            text: parts.join(" "),
            language,
        })
    }

    /// One request, sent again once if the link drops under it: the audio
    /// is all still here.
    async fn request<C: Fn() -> bool + ?Sized>(
        &self,
        samples: &[f32],
        options: &StartOptions,
        cancelled: &C,
    ) -> Result<Transcription, RemoteError> {
        match self.request_once(samples, options, cancelled).await {
            Err(RemoteError::Unreachable(why)) => {
                info!("The link dropped during a request ({why}); sending it again");
                self.request_once(samples, options, cancelled).await
            }
            result => result,
        }
    }

    async fn request_once<C: Fn() -> bool + ?Sized>(
        &self,
        samples: &[f32],
        options: &StartOptions,
        cancelled: &C,
    ) -> Result<Transcription, RemoteError> {
        let sender = self.connected(cancelled).await?;
        let (tx, mut answers) = mpsc::unbounded_channel();
        self.lock().listener = Some(tx);
        let link_gone = || RemoteError::Unreachable("the connection closed".into());
        let send = |message| sender.send(message).map_err(|_| link_gone());

        // Start, waiting out a computer that's busy for a while.
        let busy_until = Instant::now() + BUSY_WAIT;
        // From the first start on, giving up tells the other computer to stop
        // (a cancel while it's idle is ignored).
        let mut guard = CancelOnDrop {
            sender: Some(sender.clone()),
        };
        loop {
            send(ClientMessage::Start {
                output: Some(OutputMode::Phone),
                post_process: Some(false),
                options: Some(options.clone()),
            })?;
            let answer = unless_cancelled(
                tokio::time::timeout(ANSWER_WAIT, async {
                    loop {
                        match answers.recv().await {
                            // What's left of an earlier request.
                            Some(ServerMessage::State {
                                state: SessionState::Idle | SessionState::Transcribing,
                            }) => continue,
                            Some(ServerMessage::Final { .. }) => continue,
                            other => return other,
                        }
                    }
                }),
                cancelled,
            )
            .await?
            .map_err(|_| RemoteError::Unreachable("it stopped answering".into()))?;
            match answer {
                Some(ServerMessage::State {
                    state: SessionState::Recording,
                }) => break,
                Some(ServerMessage::Error {
                    code: ErrorCode::Busy,
                    ..
                }) if Instant::now() < busy_until => {
                    unless_cancelled(tokio::time::sleep(BUSY_POLL), cancelled).await?;
                }
                Some(ServerMessage::Error { code, message, .. }) => {
                    return Err(match code {
                        ErrorCode::Busy => RemoteError::Busy,
                        ErrorCode::NoModel => RemoteError::NoModel,
                        ErrorCode::Unauthorized => RemoteError::Rejected,
                        ErrorCode::UnsupportedProtocol => RemoteError::Incompatible,
                        code => RemoteError::Failed(message.unwrap_or_else(|| format!("{code:?}"))),
                    })
                }
                Some(_) => {}
                None => return Err(link_gone()),
            }
        }
        for frame in samples.chunks(FRAME_SAMPLES) {
            sender
                .send_audio(f32_to_pcm16(frame))
                .map_err(|_| link_gone())?;
        }
        send(ClientMessage::Stop)?;

        let audio = Duration::from_secs_f64(samples.len() as f64 / 16_000.0);
        let result = unless_cancelled(
            tokio::time::timeout(TRANSCRIBE_SLACK + audio * 2, async {
                loop {
                    match answers.recv().await {
                        Some(ServerMessage::Final {
                            text,
                            language,
                            language_source,
                            ..
                        }) => {
                            return Ok(Transcription {
                                text,
                                language: language_decision(language, language_source),
                            })
                        }
                        // The text came, whatever else went wrong there.
                        Some(ServerMessage::Error {
                            text: Some(text), ..
                        }) => {
                            return Ok(Transcription {
                                text,
                                language: None,
                            })
                        }
                        Some(ServerMessage::Error {
                            code: ErrorCode::TooLong,
                            ..
                        }) => continue,
                        Some(ServerMessage::Error { code, message, .. }) => {
                            return Err(match code {
                                ErrorCode::NoModel => RemoteError::NoModel,
                                code => RemoteError::Failed(
                                    message.unwrap_or_else(|| format!("{code:?}")),
                                ),
                            })
                        }
                        Some(_) => continue,
                        None => return Err(link_gone()),
                    }
                }
            }),
            cancelled,
        )
        .await;
        match result {
            Err(e) => Err(e),
            // Not worth sending again: it's there, just not answering. The
            // guard tells it to stop.
            Ok(Err(_)) => Err(RemoteError::Failed("it took too long to answer".into())),
            Ok(Ok(answer)) => {
                if !matches!(answer, Err(RemoteError::Unreachable(_))) {
                    guard.disarm();
                }
                answer
            }
        }
    }
}

impl RemoteError {
    /// What a dictation that ended on this error tells the user.
    pub fn notice_code(&self) -> NoticeCode {
        match self {
            Self::Unreachable(_) => NoticeCode::RemoteUnreachable,
            Self::Busy => NoticeCode::RemoteBusy,
            Self::NoModel => NoticeCode::RemoteNoModel,
            Self::Rejected => NoticeCode::RemoteRejected,
            Self::Incompatible => NoticeCode::RemoteIncompatible,
            Self::Failed(_) | Self::Cancelled => NoticeCode::TranscriptionFailed,
        }
    }
}

/// Why a dictation has no text, as the notice that says so.
pub struct EngineFailure {
    pub code: NoticeCode,
    pub detail: String,
}

impl EngineFailure {
    pub fn local(e: anyhow::Error) -> Self {
        Self {
            code: NoticeCode::TranscriptionFailed,
            detail: e.to_string(),
        }
    }
}

impl std::fmt::Display for EngineFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.detail)
    }
}

/// Whether this computer's own model can stand in when the other computer
/// can't be used.
pub fn can_fall_back(app: &AppHandle) -> bool {
    let settings = get_settings(app);
    settings.remote_fallback
        && !settings.selected_model.is_empty()
        && app
            .state::<Arc<crate::managers::model::ModelManager>>()
            .get_model_path(&settings.selected_model)
            .is_ok()
}

/// Transcribes a dictation's recording with the other computer, or with
/// this computer's model when that can't be used and the user allows it.
/// The flag says it fell back; the dictation says so once the text is out.
pub async fn transcribe_dictation<C: Fn() -> bool + ?Sized>(
    app: &AppHandle,
    samples: Vec<f32>,
    cancelled: &C,
) -> (Result<Transcription, EngineFailure>, bool) {
    let remote = Arc::clone(&app.state::<Arc<RemoteEngineManager>>());
    let error = match remote.transcribe(&samples, cancelled).await {
        Ok(transcription) => return (Ok(transcription), false),
        Err(RemoteError::Cancelled) => {
            return (
                Err(EngineFailure {
                    code: NoticeCode::TranscriptionFailed,
                    detail: "cancelled".into(),
                }),
                false,
            )
        }
        Err(e) => e,
    };
    if !can_fall_back(app) {
        warn!("Transcribing with the other computer failed: {error}");
        return (
            Err(EngineFailure {
                code: error.notice_code(),
                detail: error.to_string(),
            }),
            false,
        );
    }
    warn!("Transcribing with the other computer failed ({error}); using this computer's model");
    let tm = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
    tm.initiate_model_load(true);
    let result = tauri::async_runtime::spawn_blocking(move || tm.transcribe_recording(samples))
        .await
        .map_err(|e| anyhow!("transcription crashed: {e}"))
        .and_then(|result| result);
    (result.map_err(EngineFailure::local), true)
}

/// Transcribes a file job's audio with the other computer the way this
/// computer would: cut at pauses here, a request per chunk, a paragraph
/// each. Between chunks, dictation here goes first. `None` if `control`
/// stopped it.
pub fn transcribe_file(
    app: &AppHandle,
    audio: &[f32],
    control: &dyn ChunkControl,
) -> Result<Option<Transcription>, RemoteError> {
    use crate::audio_toolkit::chunking::Chunker;
    use crate::managers::audio::create_detector;

    let remote = Arc::clone(&app.state::<Arc<RemoteEngineManager>>());
    let total = audio.len();
    let backend = get_settings(app).vad_backend;
    let detector_app = app.clone();
    let mut chunker = Chunker::new(Box::new(move || create_detector(backend, &detector_app)));
    let mut pieces = audio.chunks(FILE_FEED_SAMPLES);
    let mut paragraphs = Vec::new();
    let mut language = None;
    let mut done = 0;
    control.progress(done, total);
    loop {
        if !control.proceed() {
            return Ok(None);
        }
        let (chunks, last) = match pieces.next() {
            Some(piece) => (chunker.push(piece), false),
            None => (chunker.finish(), true),
        };
        for chunk in chunks {
            if !control.proceed() {
                return Ok(None);
            }
            let cancelled = || control.stopped();
            let transcription =
                tauri::async_runtime::block_on(remote.transcribe(&chunk, &cancelled))?;
            if language.is_none() {
                language = transcription.language;
            }
            let text = transcription.text.trim().to_string();
            if !text.is_empty() {
                paragraphs.push(text);
            }
            done += chunk.len();
            control.progress(done, total);
        }
        if last {
            break;
        }
    }
    Ok(Some(Transcription {
        text: paragraphs.join("\n\n"),
        language,
    }))
}

fn client_info() -> ClientInfo {
    ClientInfo {
        device_kind: Some(device_kind::COMPUTER.to_string()),
        features: vec![feature::RAW_TEXT.to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_sources_survive_the_wire() {
        for source in [
            LanguageSource::Selected,
            LanguageSource::Model,
            LanguageSource::Detected,
            LanguageSource::Fallback,
        ] {
            let decision =
                language_decision(Some("de".into()), Some(source.as_str().into())).unwrap();
            assert_eq!(decision.source, source);
        }
        assert!(language_decision(Some("de".into()), Some("new".into())).is_none());
    }

    #[test]
    fn text_settings_go_over_and_back() {
        let mut settings = crate::settings::get_default_settings();
        settings.selected_language = "de".into();
        settings.custom_words = vec!["Handy".into()];
        settings.chinese_script = crate::settings::ChineseScript::TraditionalHk;
        settings.language_detection = crate::settings::LanguageDetectionMode::Always;
        let options = text_options(&settings);

        let mut there = crate::settings::get_default_settings();
        crate::managers::mobile::apply_text_options(&mut there, &options);
        assert_eq!(there.selected_language, "de");
        assert_eq!(there.custom_words, vec!["Handy".to_string()]);
        assert_eq!(
            there.chinese_script,
            crate::settings::ChineseScript::TraditionalHk
        );
        assert_eq!(
            there.language_detection,
            crate::settings::LanguageDetectionMode::Always
        );
    }
}

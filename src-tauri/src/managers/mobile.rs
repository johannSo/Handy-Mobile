//! Handy Mobile: paired phones dictate through this computer's model.
//!
//! A phone is a remote microphone. It streams a recording over the local
//! network (see the `handy-link` crate for the server, protocol and pairing)
//! and this manager transcribes it the way a background file job would, then
//! pastes the text here or only hands it back, as the user set.
//!
//! The paired phones and this desktop's certificate live in their own file,
//! not in the settings store: the frontend gets every setting, and none of
//! it has any business holding a private key.

use anyhow::{anyhow, Result};
use handy_link::pairing::{
    constant_time_eq, hash_secret, new_secret, random_hex, secret_matches, PairingPayload,
};
use handy_link::protocol::{ErrorCode, OutputMode, SessionState};
use handy_link::server::{
    Failure, Host, HostInfo, PairedDevice, Recording, Server, ServerOptions, Transcript,
};
use handy_link::Identity;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use crate::actions::process_transcription_output;
use crate::audio_toolkit::save_wav_file;
use crate::language_detection::LanguageDecision;
use crate::managers::history::HistoryManager;
use crate::managers::transcription::{ChunkControl, Transcription, TranscriptionManager};
use crate::notices::{self, Notice, NoticeCode, NoticeSource};
use crate::settings::{get_settings, MobileOutputMode};
use crate::utils::{self, PasteOutcome};

const STORE_FILE: &str = "mobile_link.json";
/// How long a pairing code can be scanned.
const PAIRING_WINDOW: Duration = Duration::from_secs(120);
/// How often a phone's transcription checks whether dictation here is done.
const DICTATION_POLL: Duration = Duration::from_millis(200);
const MAX_DEVICE_NAME_CHARS: usize = 40;

#[derive(Clone, Serialize, Deserialize)]
struct StoredDevice {
    id: String,
    name: String,
    /// The phone holds the secret; this is all that's kept of it.
    secret_hash: String,
    paired_at: i64,
    #[serde(default)]
    last_seen: Option<i64>,
}

#[derive(Serialize, Deserialize)]
struct LinkStore {
    /// What phones ask the network for when this desktop's address changed.
    desktop_id: String,
    identity: Identity,
    #[serde(default)]
    devices: Vec<StoredDevice>,
}

impl LinkStore {
    fn new() -> Result<Self> {
        Ok(Self {
            desktop_id: random_hex(8),
            identity: Identity::generate().map_err(|e| anyhow!(e))?,
            devices: Vec::new(),
        })
    }
}

struct PendingPairing {
    token: String,
    expires: Instant,
}

/// A paired phone, as the Mobile tab lists it.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct MobileDevice {
    pub id: String,
    pub name: String,
    /// Unix seconds.
    pub paired_at: i64,
    pub last_seen: Option<i64>,
    pub connected: bool,
    /// Recording or being transcribed right now.
    pub active: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct MobileStatus {
    /// Listening for phones.
    pub running: bool,
    pub port: u16,
    /// The addresses a phone on the same network can reach this computer at.
    pub addresses: Vec<String>,
    /// Why the server isn't running although it's turned on.
    pub error: Option<String>,
    pub devices: Vec<MobileDevice>,
}

/// What the pairing dialog shows.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct MobilePairing {
    /// The code itself, for typing in where a camera can't be used.
    pub code: String,
    /// The same code as a QR code.
    pub qr_svg: String,
    pub expires_in_secs: u32,
}

/// Something on the Mobile tab is out of date: a phone paired, connected,
/// left, or started or finished dictating.
#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct MobileLinkChanged {
    /// The phone that just paired, so the pairing dialog can close itself.
    pub paired_device: Option<String>,
}

struct State {
    /// `None` until first needed: most people never turn this on, and a
    /// certificate nobody uses needn't exist.
    store: Option<LinkStore>,
    server: Option<Server>,
    /// The port the running server was asked for.
    port: u16,
    error: Option<String>,
    pairing: Option<PendingPairing>,
    /// The phone recording or being transcribed.
    active: Option<String>,
}

pub struct MobileLinkManager {
    app: AppHandle,
    state: Mutex<State>,
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn clean_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_DEVICE_NAME_CHARS)
        .collect();
    cleaned.trim().to_string()
}

/// Interfaces that go nowhere a phone could be: containers and VM bridges.
fn is_virtual_interface(name: &str) -> bool {
    [
        "docker", "br-", "veth", "virbr", "vboxnet", "vmnet", "lxc", "podman", "cni",
    ]
    .iter()
    .any(|prefix| name.starts_with(prefix))
}

/// This computer's IPv4 addresses a phone could reach, the one the default
/// route uses first.
fn reachable_addresses() -> Vec<IpAddr> {
    let mut addresses: Vec<Ipv4Addr> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|interface| !interface.is_loopback() && !is_virtual_interface(&interface.name))
        .filter_map(|interface| match interface.ip() {
            IpAddr::V4(ip) if !ip.is_link_local() => Some(ip),
            _ => None,
        })
        .collect();
    addresses.sort();
    addresses.dedup();
    if let Some(primary) = handy_link::discovery::local_ipv4() {
        addresses.retain(|ip| *ip != primary);
        addresses.insert(0, primary);
    }
    addresses.into_iter().map(IpAddr::V4).collect()
}

fn qr_svg(code: &str) -> Result<String> {
    use qrcode::render::svg;
    use qrcode::{EcLevel, QrCode};
    // Low error correction keeps the modules large: this is read off a
    // screen from arm's length, not off a scuffed label.
    let qr = QrCode::with_error_correction_level(code.as_bytes(), EcLevel::L)
        .map_err(|e| anyhow!("Failed to draw the pairing code: {e}"))?;
    Ok(qr
        .render::<svg::Color>()
        .min_dimensions(240, 240)
        .quiet_zone(true)
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"))
        .build())
}

impl MobileLinkManager {
    pub fn new(app: &AppHandle) -> Arc<Self> {
        Arc::new(Self {
            app: app.clone(),
            state: Mutex::new(State {
                store: None,
                server: None,
                port: 0,
                error: None,
                pairing: None,
                active: None,
            }),
        })
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn store_path(&self) -> Result<PathBuf> {
        Ok(crate::portable::app_data_dir(&self.app)?.join(STORE_FILE))
    }

    /// The store, read from disk or created on first use.
    fn store<'a>(&self, state: &'a mut State) -> Result<&'a mut LinkStore> {
        if state.store.is_none() {
            let path = self.store_path()?;
            let loaded = match std::fs::read_to_string(&path) {
                Ok(json) => match serde_json::from_str::<LinkStore>(&json) {
                    Ok(store) if store.identity.is_valid() => Some(store),
                    Ok(_) => {
                        warn!("The stored certificate is unusable; every phone must pair again");
                        None
                    }
                    Err(e) => {
                        warn!(
                            "{} is unreadable ({e}); every phone must pair again",
                            path.display()
                        );
                        None
                    }
                },
                Err(_) => None,
            };
            let store = match loaded {
                Some(store) => store,
                None => {
                    let store = LinkStore::new()?;
                    self.save(&store)?;
                    store
                }
            };
            state.store = Some(store);
        }
        Ok(state.store.as_mut().expect("just set"))
    }

    fn save(&self, store: &LinkStore) -> Result<()> {
        let path = self.store_path()?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // Written beside it and renamed over it, so a crash mid-write can't
        // leave half a file and unpair every phone.
        let tmp = path.with_extension("json.tmp");
        {
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create(true).truncate(true);
            // The private key is in here: nobody else on this computer reads it.
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

    fn changed(&self, paired_device: Option<String>) {
        if let Err(e) = (MobileLinkChanged { paired_device }).emit(&self.app) {
            warn!("Failed to emit mobile-link-changed: {e}");
        }
    }

    /// Starts, stops or restarts the server to match the settings. Call at
    /// startup and whenever `mobile_enabled` or `mobile_port` change.
    pub fn apply_settings(self: &Arc<Self>) {
        let settings = get_settings(&self.app);
        let mut state = self.lock();
        let wanted = settings.mobile_enabled.then_some(settings.mobile_port);
        let running = state.server.as_ref().map(|_| state.port);
        if wanted == running && (wanted.is_none() || state.error.is_none()) {
            return;
        }

        // Dropping the server disconnects every phone.
        state.server = None;
        state.pairing = None;
        state.error = None;
        let Some(port) = wanted else {
            drop(state);
            notices::clear(&self.app, NoticeSource::Mobile);
            info!("Stopped listening for phones");
            self.changed(None);
            return;
        };

        let started = self.store(&mut state).and_then(|store| {
            let identity = store.identity.clone();
            let desktop_id = store.desktop_id.clone();
            let host: Arc<dyn Host> = Arc::clone(self) as Arc<dyn Host>;
            tauri::async_runtime::block_on(Server::bind(
                SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port),
                &identity,
                host,
                ServerOptions {
                    discoverable_as: Some(desktop_id),
                    ..ServerOptions::default()
                },
            ))
            .map_err(|e| anyhow!(e))
        });
        match started {
            Ok(server) => {
                state.server = Some(server);
                state.port = port;
                drop(state);
                notices::clear(&self.app, NoticeSource::Mobile);
            }
            Err(e) => {
                error!("Failed to start listening for phones: {e}");
                state.port = port;
                state.error = Some(e.to_string());
                drop(state);
                notices::raise(
                    &self.app,
                    Notice::new(NoticeCode::MobileServerFailed, NoticeSource::Mobile)
                        .with_detail(e.to_string()),
                );
            }
        }
        self.changed(None);
    }

    pub fn status(&self) -> MobileStatus {
        let settings = get_settings(&self.app);
        let state = self.lock();
        let connected = state
            .server
            .as_ref()
            .map(Server::connected_devices)
            .unwrap_or_default();
        let devices = state
            .store
            .as_ref()
            .map(|store| {
                store
                    .devices
                    .iter()
                    .map(|device| MobileDevice {
                        id: device.id.clone(),
                        name: device.name.clone(),
                        paired_at: device.paired_at,
                        last_seen: device.last_seen,
                        connected: connected.contains(&device.id),
                        active: state.active.as_deref() == Some(&device.id),
                    })
                    .collect()
            })
            .unwrap_or_default();
        MobileStatus {
            running: state.server.is_some(),
            port: settings.mobile_port,
            addresses: reachable_addresses()
                .iter()
                .map(ToString::to_string)
                .collect(),
            error: state.error.clone(),
            devices,
        }
    }

    /// Loads the paired phones for a status that is asked for before the
    /// server ever ran (the tab lists them even while the feature is off).
    pub fn load_devices(&self) {
        // Only if the file exists: looking at the tab mustn't create a key.
        if self.store_path().is_ok_and(|path| path.exists()) {
            let mut state = self.lock();
            if let Err(e) = self.store(&mut state) {
                warn!("Failed to read the paired phones: {e}");
            }
        }
    }

    /// A fresh one-time code for a phone to scan. Replaces the previous one.
    pub fn start_pairing(&self) -> Result<MobilePairing> {
        let mut state = self.lock();
        let Some(server) = state.server.as_ref() else {
            return Err(anyhow!("Turn on Handy Mobile first"));
        };
        let port = server.local_addr().port();
        let addresses = reachable_addresses();
        if addresses.is_empty() {
            return Err(anyhow!("This computer isn't connected to a network"));
        }
        let token = random_hex(16);
        let store = self.store(&mut state)?;
        let code = PairingPayload {
            desktop_id: store.desktop_id.clone(),
            desktop_name: desktop_name(),
            addresses,
            port,
            fingerprint: store.identity.fingerprint(),
            token: token.clone(),
        }
        .encode();
        let qr_svg = qr_svg(&code)?;
        state.pairing = Some(PendingPairing {
            token,
            expires: Instant::now() + PAIRING_WINDOW,
        });
        Ok(MobilePairing {
            code,
            qr_svg,
            expires_in_secs: PAIRING_WINDOW.as_secs() as u32,
        })
    }

    pub fn cancel_pairing(&self) {
        self.lock().pairing = None;
    }

    pub fn rename_device(&self, id: &str, name: &str) -> Result<()> {
        let name = clean_name(name);
        if name.is_empty() {
            return Err(anyhow!("A phone needs a name"));
        }
        {
            let mut state = self.lock();
            let store = self.store(&mut state)?;
            let device = store
                .devices
                .iter_mut()
                .find(|device| device.id == id)
                .ok_or_else(|| anyhow!("That phone isn't paired"))?;
            device.name = name;
            self.save(store)?;
        }
        self.changed(None);
        Ok(())
    }

    /// Unpairs a phone and drops its connection. Its secret stops working.
    pub fn remove_device(&self, id: &str) -> Result<()> {
        {
            let mut state = self.lock();
            let store = self.store(&mut state)?;
            store.devices.retain(|device| device.id != id);
            self.save(store)?;
            if let Some(server) = state.server.as_ref() {
                server.disconnect(id);
            }
        }
        self.changed(None);
        Ok(())
    }

    fn device_name(&self, id: &str) -> Option<String> {
        self.lock().store.as_ref().and_then(|store| {
            store
                .devices
                .iter()
                .find(|device| device.id == id)
                .map(|device| device.name.clone())
        })
    }

    /// Transcribes a phone's recording and delivers the text. Runs on a
    /// blocking thread.
    fn run(&self, recording: Recording) -> Result<Transcript, Failure> {
        let Recording {
            device_id,
            samples,
            output,
            post_process,
            cancelled,
        } = recording;
        if samples.is_empty() {
            return Ok(Transcript {
                text: String::new(),
                pasted: false,
            });
        }

        let app = &self.app;
        let settings = get_settings(app);
        let tm = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
        let hm = Arc::clone(&app.state::<Arc<HistoryManager>>());
        let paste = match output {
            Some(OutputMode::Desktop) => true,
            Some(OutputMode::Phone) => false,
            None => settings.mobile_output_mode == MobileOutputMode::Desktop,
        };
        // Post Process has to be set up here either way.
        let post_process =
            settings.post_process_enabled && post_process.unwrap_or(settings.mobile_post_process);

        // The recording goes to history like any other, so a failed one can
        // be transcribed again from there.
        let file_name = format!("handy-mobile-{}.wav", chrono::Utc::now().timestamp_millis());
        let wav_saved = match save_wav_file(hm.recordings_dir().join(&file_name), &samples) {
            Ok(()) => true,
            Err(e) => {
                error!("Failed to save a phone's recording: {e}");
                false
            }
        };
        let save_history = |text: String,
                            post_processed: Option<String>,
                            prompt: Option<String>,
                            language: Option<LanguageDecision>| {
            if !wav_saved {
                return;
            }
            if let Err(e) = hm.save_entry(
                file_name.clone(),
                text,
                post_process,
                post_processed,
                prompt,
                language,
            ) {
                error!("Failed to save a phone's dictation to history: {e}");
            }
        };
        let discard_recording = || {
            if wav_saved {
                let _ = std::fs::remove_file(hm.recordings_dir().join(&file_name));
            }
        };
        let fail = |e: &dyn std::fmt::Display| {
            notices::raise(
                app,
                Notice::new(NoticeCode::MobileTranscriptionFailed, NoticeSource::Mobile)
                    .with_detail(e.to_string()),
            );
            Failure::new(ErrorCode::TranscriptionFailed).with_message(e.to_string())
        };

        let control = Control {
            app,
            cancelled: &cancelled,
        };
        let transcribed = tm.transcribe_in_chunks(samples, &control);
        // A dictation under way unloads the model itself when it ends.
        if !crate::tray::dictation_in_progress(app) {
            tm.maybe_unload_immediately("phone transcription");
        }
        let Transcription { text, language } = match transcribed {
            Ok(Some(transcription)) => transcription,
            // Cancelled: the phone isn't waiting for an answer.
            Ok(None) => {
                discard_recording();
                return Ok(Transcript {
                    text: String::new(),
                    pasted: false,
                });
            }
            Err(e) => {
                warn!("Transcribing a phone's recording failed: {e}");
                save_history(String::new(), None, None, None);
                return Err(fail(&e));
            }
        };
        if text.trim().is_empty() {
            debug!("A phone's recording had no speech");
            discard_recording();
            notices::clear(app, NoticeSource::Mobile);
            return Ok(Transcript {
                text: String::new(),
                pasted: false,
            });
        }

        let processed =
            tauri::async_runtime::block_on(process_transcription_output(app, &text, post_process));
        if cancelled.load(Ordering::Acquire) {
            discard_recording();
            return Ok(Transcript {
                text: String::new(),
                pasted: false,
            });
        }
        save_history(
            text,
            processed.post_processed_text.clone(),
            processed.post_process_prompt.clone(),
            language,
        );
        // The user's hook only changes what is delivered; history above
        // keeps the text as produced.
        let final_text = tauri::async_runtime::block_on(crate::transcription_hook::apply(
            app,
            processed.final_text,
        ));
        if final_text.is_empty() || cancelled.load(Ordering::Acquire) {
            return Ok(Transcript {
                text: String::new(),
                pasted: false,
            });
        }

        // Clear first, so a warning about this dictation survives it.
        notices::clear(app, NoticeSource::Mobile);
        if processed.post_process_failed {
            notices::raise(
                app,
                Notice::new(NoticeCode::PostProcessFailed, NoticeSource::Mobile),
            );
        }
        if !paste {
            return Ok(Transcript {
                text: final_text,
                pasted: false,
            });
        }

        debug!("Pasting what phone {device_id} dictated");
        let pasted = match utils::paste(final_text.clone(), app.clone()) {
            Ok(PasteOutcome::Pasted) => Ok(()),
            Ok(PasteOutcome::Blocked) => Err(NoticeCode::PasteBlockedElevated),
            Err(e) => {
                error!("Failed to paste a phone's dictation: {e}");
                Err(NoticeCode::PasteFailed)
            }
        };
        match pasted {
            Ok(()) => Ok(Transcript {
                text: final_text,
                pasted: true,
            }),
            Err(code) => {
                notices::raise(app, Notice::new(code, NoticeSource::Mobile));
                // The phone still gets the text; only the paste was lost.
                Err(Failure {
                    code: ErrorCode::PasteFailed,
                    message: None,
                    text: Some(final_text),
                })
            }
        }
    }
}

fn desktop_name() -> String {
    let name = gethostname::gethostname()
        .to_string_lossy()
        .trim()
        .to_string();
    if name.is_empty() {
        "Handy".to_string()
    } else {
        name
    }
}

impl Host for MobileLinkManager {
    fn info(&self) -> HostInfo {
        let settings = get_settings(&self.app);
        HostInfo {
            name: desktop_name(),
            version: self.app.package_info().version.to_string(),
            default_output: match settings.mobile_output_mode {
                MobileOutputMode::Desktop => OutputMode::Desktop,
                MobileOutputMode::Phone => OutputMode::Phone,
            },
        }
    }

    fn pair(&self, token: &str, device_name: &str) -> Option<PairedDevice> {
        let paired = {
            let mut state = self.lock();
            let pending = state.pairing.as_ref()?;
            if pending.expires < Instant::now() {
                state.pairing = None;
                return None;
            }
            if !constant_time_eq(pending.token.as_bytes(), token.as_bytes()) {
                return None;
            }
            // Good for one phone.
            state.pairing = None;

            let device_id = random_hex(8);
            let secret = new_secret();
            let store = self.store(&mut state).ok()?;
            store.devices.push(StoredDevice {
                id: device_id.clone(),
                name: device_name.to_string(),
                secret_hash: hash_secret(&secret),
                paired_at: now(),
                last_seen: Some(now()),
            });
            if let Err(e) = self.save(store) {
                error!("Failed to save a newly paired phone: {e}");
                store.devices.retain(|device| device.id != device_id);
                return None;
            }
            info!("Paired with phone {device_id} ({device_name})");
            PairedDevice { device_id, secret }
        };
        self.changed(Some(paired.device_id.clone()));
        Some(paired)
    }

    fn authenticate(&self, device_id: &str, secret: &str) -> bool {
        let mut state = self.lock();
        let Ok(store) = self.store(&mut state) else {
            return false;
        };
        let Some(device) = store
            .devices
            .iter_mut()
            .find(|device| device.id == device_id)
        else {
            return false;
        };
        if !secret_matches(secret, &device.secret_hash) {
            return false;
        }
        device.last_seen = Some(now());
        if let Err(e) = self.save(store) {
            warn!("Failed to note when a phone was last seen: {e}");
        }
        true
    }

    fn connected(&self, _device_id: &str, _peer: SocketAddr) {
        self.changed(None);
    }

    fn disconnected(&self, _device_id: &str) {
        self.changed(None);
    }

    fn begin(&self, device_id: &str) -> Result<(), ErrorCode> {
        // Both would paste at the same cursor.
        if crate::tray::dictation_in_progress(&self.app) {
            return Err(ErrorCode::Busy);
        }
        if get_settings(&self.app).selected_model.is_empty() {
            return Err(ErrorCode::NoModel);
        }
        let tm = self.app.state::<Arc<TranscriptionManager>>();
        tm.set_mobile_active(true);
        // Load while the user talks, so the text isn't kept waiting for it.
        tm.initiate_model_load(false);
        debug!(
            "Phone {} started dictating",
            self.device_name(device_id).as_deref().unwrap_or(device_id)
        );
        Ok(())
    }

    fn state_changed(&self, device_id: &str, state: SessionState) {
        let idle = state == SessionState::Idle;
        self.lock().active = (!idle).then(|| device_id.to_string());
        if idle {
            self.app
                .state::<Arc<TranscriptionManager>>()
                .set_mobile_active(false);
        }
        self.changed(None);
    }

    fn transcribe(&self, recording: Recording) -> Result<Transcript, Failure> {
        self.run(recording)
    }
}

struct Control<'a> {
    app: &'a AppHandle,
    cancelled: &'a AtomicBool,
}

impl ChunkControl for Control<'_> {
    fn proceed(&self) -> bool {
        // Dictation at this computer goes first, as it does for file jobs:
        // a chunk started while its stream is open would end that stream.
        loop {
            if self.cancelled.load(Ordering::Acquire) {
                return false;
            }
            if !crate::tray::dictation_in_progress(self.app) {
                return true;
            }
            thread::sleep(DICTATION_POLL);
        }
    }

    fn progress(&self, _done: usize, _total: usize) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pairing_code_fits_a_qr_code() {
        let code = PairingPayload {
            desktop_id: random_hex(8),
            desktop_name: "a-rather-long-hostname.example.internal".into(),
            addresses: vec![
                "192.168.178.123".parse().unwrap(),
                "10.200.100.200".parse().unwrap(),
                "100.101.102.103".parse().unwrap(),
            ],
            port: 47823,
            fingerprint: "ab".repeat(32),
            token: random_hex(16),
        }
        .encode();
        let svg = qr_svg(&code).unwrap();
        assert!(svg.starts_with("<?xml") || svg.starts_with("<svg"));
        assert_eq!(PairingPayload::decode(&code).unwrap().port, 47823);
    }

    #[test]
    fn container_interfaces_are_left_out() {
        assert!(is_virtual_interface("docker0"));
        assert!(is_virtual_interface("br-3f2a"));
        assert!(!is_virtual_interface("wlan0"));
        assert!(!is_virtual_interface("tailscale0"));
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  Pixel\n"), "Pixel");
        assert_eq!(clean_name("\t"), "");
    }
}

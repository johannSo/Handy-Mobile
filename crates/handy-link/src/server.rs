//! The desktop side of the link: accepts phones, pairs and authenticates
//! them, collects each recording and hands it to the [`Host`].
//!
//! Nothing here knows about Handy's models or settings; the desktop app
//! implements [`Host`] to supply them, which is also what lets the tests run
//! the whole exchange without it.

use crate::discovery;
use crate::protocol::{
    pcm16_to_f32, ClientMessage, ErrorCode, OutputMode, ServerMessage, SessionState,
    PROTOCOL_VERSION,
};
use crate::tls::Identity;
use futures_util::{SinkExt, StreamExt};
use log::{debug, info, warn};
use std::collections::HashMap;
use std::future::pending;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, oneshot, Semaphore};
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tokio_rustls::TlsAcceptor;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

/// How long a phone has for each step before it is paired or authenticated.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// A connected phone pings well inside this; silence means it's gone.
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
/// Failed pairings or logins from one address before it has to wait.
const MAX_FAILURES: u32 = 5;
const FAILURE_WINDOW: Duration = Duration::from_secs(60);
/// Connections that haven't authenticated yet, across all addresses.
const MAX_PENDING: usize = 16;
/// No control message is near this, and audio frames are a fraction of it.
const MAX_MESSAGE_BYTES: usize = 1 << 20;
const MAX_DEVICE_NAME_CHARS: usize = 40;

/// The longest recording kept: 30 minutes, about 115 MB of samples.
pub const DEFAULT_MAX_RECORDING_SAMPLES: usize = 30 * 60 * crate::SAMPLE_RATE as usize;

pub struct HostInfo {
    pub name: String,
    pub version: String,
    pub default_output: OutputMode,
}

/// A phone that just paired. The secret goes to the phone once and is never
/// seen in the clear again; the host keeps only its hash.
pub struct PairedDevice {
    pub device_id: String,
    pub secret: String,
}

/// A finished recording to transcribe.
pub struct Recording {
    pub device_id: String,
    pub samples: Vec<f32>,
    /// `None`: use what the desktop is set to.
    pub output: Option<OutputMode>,
    pub post_process: Option<bool>,
    /// Set when the phone cancels. The host should stop when it sees this,
    /// and must not paste once it is set.
    pub cancelled: Arc<AtomicBool>,
}

pub struct Transcript {
    pub text: String,
    pub pasted: bool,
}

pub struct Failure {
    pub code: ErrorCode,
    pub message: Option<String>,
    /// The transcript, when the failure came after transcribing.
    pub text: Option<String>,
}

impl Failure {
    pub fn new(code: ErrorCode) -> Self {
        Self {
            code,
            message: None,
            text: None,
        }
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}

/// What the server needs from the app it runs in. Methods are called from
/// async tasks and must return quickly, except [`Host::transcribe`], which
/// runs on a blocking thread.
pub trait Host: Send + Sync + 'static {
    fn info(&self) -> HostInfo;
    /// Redeems a pairing token, or `None` if it's wrong, used or expired.
    fn pair(&self, token: &str, device_name: &str) -> Option<PairedDevice>;
    fn authenticate(&self, device_id: &str, secret: &str) -> bool;
    fn connected(&self, _device_id: &str, _peer: SocketAddr) {}
    fn disconnected(&self, _device_id: &str) {}
    /// A phone wants to record. An error refuses it.
    fn begin(&self, _device_id: &str) -> Result<(), ErrorCode> {
        Ok(())
    }
    /// The phone's session changed state. `Idle` ends what `begin` started,
    /// however it ended.
    fn state_changed(&self, _device_id: &str, _state: SessionState) {}
    fn transcribe(&self, recording: Recording) -> Result<Transcript, Failure>;
}

#[derive(Clone)]
enum Kick {
    /// This device's connections on one channel, or on all of them, except
    /// the connection with this id.
    Device {
        device_id: String,
        channel: Option<String>,
        except: u64,
    },
    All,
}

struct Shared {
    host: Arc<dyn Host>,
    /// The device recording or being transcribed. One at a time: there is
    /// one model and one cursor to paste at.
    active: Mutex<Option<String>>,
    failures: Mutex<HashMap<IpAddr, (u32, Instant)>>,
    connections: Mutex<HashMap<u64, String>>,
    next_connection: AtomicU64,
    pending: Arc<Semaphore>,
    kick: broadcast::Sender<Kick>,
    max_recording_samples: usize,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Shared {
    fn rate_limited(&self, ip: IpAddr) -> bool {
        let mut failures = lock(&self.failures);
        failures.retain(|_, (_, since)| since.elapsed() < FAILURE_WINDOW);
        failures
            .get(&ip)
            .is_some_and(|(count, _)| *count >= MAX_FAILURES)
    }

    fn record_failure(&self, ip: IpAddr) {
        let mut failures = lock(&self.failures);
        let entry = failures.entry(ip).or_insert((0, Instant::now()));
        entry.0 += 1;
    }
}

/// Holds the one session slot. Dropping it frees the slot and tells the host
/// the session is over, wherever that happens: a cancel, a dropped
/// connection, or the end of a transcription nobody is waiting for any more.
struct Slot {
    shared: Arc<Shared>,
    device_id: String,
}

impl Slot {
    fn claim(shared: &Arc<Shared>, device_id: &str) -> Result<Self, ErrorCode> {
        let mut active = lock(&shared.active);
        if active.is_some() {
            return Err(ErrorCode::Busy);
        }
        shared.host.begin(device_id)?;
        *active = Some(device_id.to_string());
        Ok(Self {
            shared: Arc::clone(shared),
            device_id: device_id.to_string(),
        })
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        *lock(&self.shared.active) = None;
        self.shared
            .host
            .state_changed(&self.device_id, SessionState::Idle);
    }
}

/// A running server. Dropping it stops listening and disconnects every phone.
pub struct Server {
    shared: Arc<Shared>,
    local_addr: SocketAddr,
    accept: JoinHandle<()>,
    discovery: Option<JoinHandle<()>>,
}

pub struct ServerOptions {
    /// Answer phones looking for this desktop id (see [`discovery`]).
    pub discoverable_as: Option<String>,
    pub max_recording_samples: usize,
}

impl Default for ServerOptions {
    fn default() -> Self {
        Self {
            discoverable_as: None,
            max_recording_samples: DEFAULT_MAX_RECORDING_SAMPLES,
        }
    }
}

impl Server {
    /// Starts listening on `addr`. Must be called inside a tokio runtime.
    pub async fn bind(
        addr: SocketAddr,
        identity: &Identity,
        host: Arc<dyn Host>,
        options: ServerOptions,
    ) -> Result<Self, String> {
        let acceptor = TlsAcceptor::from(identity.server_config()?);
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| format!("Can't listen on {addr}: {e}"))?;
        let local_addr = listener.local_addr().map_err(|e| e.to_string())?;
        let (kick, _) = broadcast::channel(16);
        let shared = Arc::new(Shared {
            host,
            active: Mutex::new(None),
            failures: Mutex::new(HashMap::new()),
            connections: Mutex::new(HashMap::new()),
            next_connection: AtomicU64::new(1),
            pending: Arc::new(Semaphore::new(MAX_PENDING)),
            kick,
            max_recording_samples: options.max_recording_samples,
        });

        let discovery = options.discoverable_as.map(|desktop_id| {
            let port = local_addr.port();
            tokio::spawn(async move {
                // Not being findable costs only the reconnect after an
                // address change; the server still works.
                if let Err(e) = discovery::respond(desktop_id, port).await {
                    warn!("Phones can't look this desktop up on the network: {e}");
                }
            })
        });

        let accept = tokio::spawn({
            let shared = Arc::clone(&shared);
            async move {
                loop {
                    let (tcp, peer) = match listener.accept().await {
                        Ok(accepted) => accepted,
                        Err(e) => {
                            warn!("Failed to accept a connection: {e}");
                            tokio::time::sleep(Duration::from_millis(200)).await;
                            continue;
                        }
                    };
                    let _ = tcp.set_nodelay(true);
                    let shared = Arc::clone(&shared);
                    let acceptor = acceptor.clone();
                    tokio::spawn(async move {
                        if let Err(e) = serve(shared, acceptor, tcp, peer).await {
                            debug!("Connection from {peer} ended: {e}");
                        }
                    });
                }
            }
        });

        info!("Listening for phones on {local_addr}");
        Ok(Self {
            shared,
            local_addr,
            accept,
            discovery,
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// The ids of the phones connected right now.
    pub fn connected_devices(&self) -> Vec<String> {
        let mut devices: Vec<String> = lock(&self.shared.connections).values().cloned().collect();
        devices.sort();
        devices.dedup();
        devices
    }

    /// Drops a phone's connections, such as after it was unpaired.
    pub fn disconnect(&self, device_id: &str) {
        let _ = self.shared.kick.send(Kick::Device {
            device_id: device_id.to_string(),
            channel: None,
            except: 0,
        });
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.accept.abort();
        if let Some(discovery) = &self.discovery {
            discovery.abort();
        }
        let _ = self.shared.kick.send(Kick::All);
    }
}

type Ws<S> = WebSocketStream<S>;

async fn send<S>(ws: &mut Ws<S>, message: &ServerMessage) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let json = serde_json::to_string(message).map_err(|e| e.to_string())?;
    timeout(WRITE_TIMEOUT, ws.send(Message::text(json)))
        .await
        .map_err(|_| "the phone stopped reading".to_string())?
        .map_err(|e| e.to_string())
}

/// Says why and hangs up.
async fn refuse<S>(ws: &mut Ws<S>, code: ErrorCode) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    send(ws, &ServerMessage::error(code)).await?;
    let _ = timeout(WRITE_TIMEOUT, ws.close(None)).await;
    Ok(())
}

fn clean_device_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_DEVICE_NAME_CHARS)
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        "Phone".to_string()
    } else {
        cleaned.to_string()
    }
}

/// The next control message, skipping what isn't one. `Ok(None)`: closed.
async fn next_message<S>(ws: &mut Ws<S>) -> Result<Option<Incoming>, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    loop {
        match ws.next().await {
            None | Some(Ok(Message::Close(_))) => return Ok(None),
            Some(Ok(Message::Text(text))) => {
                return match serde_json::from_str(text.as_str()) {
                    Ok(message) => Ok(Some(Incoming::Control(message))),
                    Err(_) => Ok(Some(Incoming::Unknown)),
                }
            }
            Some(Ok(Message::Binary(bytes))) => return Ok(Some(Incoming::Audio(bytes.to_vec()))),
            Some(Ok(_)) => {}
            Some(Err(e)) => return Err(e.to_string()),
        }
    }
}

enum Incoming {
    Control(ClientMessage),
    Audio(Vec<u8>),
    /// JSON this version doesn't know. A newer phone may send it; ignoring
    /// it keeps the two compatible.
    Unknown,
}

async fn serve<S>(
    shared: Arc<Shared>,
    acceptor: TlsAcceptor,
    tcp: S,
    peer: SocketAddr,
) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (mut ws, device_id, channel) = {
        // Released once the phone has proven itself: strangers can hold only
        // so many connections open, and none for long.
        let _pending = Arc::clone(&shared.pending)
            .try_acquire_owned()
            .map_err(|_| "too many connections are waiting to authenticate".to_string())?;

        let tls = timeout(HANDSHAKE_TIMEOUT, acceptor.accept(tcp))
            .await
            .map_err(|_| "TLS handshake timed out".to_string())?
            .map_err(|e| format!("TLS handshake failed: {e}"))?;
        let mut config = WebSocketConfig::default();
        config.max_message_size = Some(MAX_MESSAGE_BYTES);
        config.max_frame_size = Some(MAX_MESSAGE_BYTES);
        let mut ws = timeout(
            HANDSHAKE_TIMEOUT,
            tokio_tungstenite::accept_async_with_config(tls, Some(config)),
        )
        .await
        .map_err(|_| "WebSocket handshake timed out".to_string())?
        .map_err(|e| format!("WebSocket handshake failed: {e}"))?;

        if shared.rate_limited(peer.ip()) {
            return refuse(&mut ws, ErrorCode::RateLimited).await;
        }

        let first = timeout(HANDSHAKE_TIMEOUT, next_message(&mut ws))
            .await
            .map_err(|_| "the phone never said who it is".to_string())??;
        let info = shared.host.info();
        let identified = match first {
            Some(Incoming::Control(
                ClientMessage::Pair { protocol, .. } | ClientMessage::Auth { protocol, .. },
            )) if protocol != PROTOCOL_VERSION => {
                return refuse(&mut ws, ErrorCode::UnsupportedProtocol).await;
            }
            Some(Incoming::Control(ClientMessage::Pair {
                token, device_name, ..
            })) => {
                let Some(paired) = shared.host.pair(&token, &clean_device_name(&device_name))
                else {
                    shared.record_failure(peer.ip());
                    return refuse(&mut ws, ErrorCode::InvalidToken).await;
                };
                send(
                    &mut ws,
                    &ServerMessage::Paired {
                        device_id: paired.device_id.clone(),
                        secret: paired.secret,
                        desktop_name: info.name,
                        desktop_version: info.version,
                        default_output: info.default_output,
                    },
                )
                .await?;
                (paired.device_id, String::new())
            }
            Some(Incoming::Control(ClientMessage::Auth {
                device_id,
                secret,
                channel,
                ..
            })) => {
                if !shared.host.authenticate(&device_id, &secret) {
                    shared.record_failure(peer.ip());
                    return refuse(&mut ws, ErrorCode::Unauthorized).await;
                }
                send(
                    &mut ws,
                    &ServerMessage::Ready {
                        desktop_name: info.name,
                        desktop_version: info.version,
                        default_output: info.default_output,
                    },
                )
                .await?;
                (device_id, channel)
            }
            None => return Ok(()),
            Some(_) => return refuse(&mut ws, ErrorCode::Unauthorized).await,
        };
        (ws, identified.0, identified.1)
    };

    let connection = shared.next_connection.fetch_add(1, Ordering::Relaxed);
    // Subscribe before announcing, so no kick meant for this connection can
    // slip between the two.
    let mut kicks = shared.kick.subscribe();
    // A phone that reconnects replaces its old connection, which may still
    // look alive from here. Only on the same channel: the phone's app and
    // its keyboard each hold a connection of their own.
    let _ = shared.kick.send(Kick::Device {
        device_id: device_id.clone(),
        channel: Some(channel.clone()),
        except: connection,
    });
    lock(&shared.connections).insert(connection, device_id.clone());
    shared.host.connected(&device_id, peer);
    info!("Phone {device_id} connected from {peer}");

    let result = session(
        &shared, &mut ws, &device_id, &channel, connection, &mut kicks,
    )
    .await;

    lock(&shared.connections).remove(&connection);
    // Only when its last connection goes: a replaced one isn't a departure.
    if !lock(&shared.connections)
        .values()
        .any(|device| device == &device_id)
    {
        shared.host.disconnected(&device_id);
    }
    info!("Phone {device_id} disconnected");
    let _ = timeout(WRITE_TIMEOUT, ws.close(None)).await;
    result
}

enum Phase {
    Idle,
    Recording {
        slot: Slot,
        samples: Vec<f32>,
        output: Option<OutputMode>,
        post_process: Option<bool>,
    },
    Transcribing {
        cancelled: Arc<AtomicBool>,
        done: oneshot::Receiver<Result<Transcript, Failure>>,
    },
}

async fn session<S>(
    shared: &Arc<Shared>,
    ws: &mut Ws<S>,
    device_id: &str,
    channel: &str,
    connection: u64,
    kicks: &mut broadcast::Receiver<Kick>,
) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut phase = Phase::Idle;
    loop {
        let finished = async {
            match &mut phase {
                Phase::Transcribing { done, .. } => done.await,
                _ => pending().await,
            }
        };
        tokio::select! {
            kick = kicks.recv() => match kick {
                Ok(Kick::Device { device_id: kicked, channel: kicked_channel, except })
                    if kicked != device_id
                        || except == connection
                        || kicked_channel.as_deref().is_some_and(|kicked| kicked != channel) => {}
                // Lagging behind the kicks means one may have been for this
                // connection; the phone reconnects if it wasn't.
                _ => return Ok(()),
            },
            result = finished => {
                let Phase::Transcribing { cancelled, .. } = &phase else {
                    unreachable!("only a transcription finishes");
                };
                if !cancelled.load(Ordering::Acquire) {
                    let message = match result {
                        Ok(Ok(Transcript { text, pasted })) => ServerMessage::Final { text, pasted },
                        Ok(Err(Failure { code, message, text })) => {
                            ServerMessage::Error { code, message, text }
                        }
                        // The transcription task died without an answer.
                        Err(_) => ServerMessage::error(ErrorCode::TranscriptionFailed),
                    };
                    send(ws, &message).await?;
                }
                phase = Phase::Idle;
                send(ws, &ServerMessage::State { state: SessionState::Idle }).await?;
            },
            incoming = timeout(IDLE_TIMEOUT, next_message(ws)) => {
                let incoming = incoming.map_err(|_| "the phone went silent".to_string())??;
                let Some(incoming) = incoming else {
                    return Ok(());
                };
                match incoming {
                    Incoming::Unknown => {}
                    Incoming::Audio(bytes) => {
                        let Phase::Recording { samples, .. } = &mut phase else {
                            // Frames still in flight after a stop or cancel.
                            continue;
                        };
                        pcm16_to_f32(&bytes, samples);
                        if samples.len() >= shared.max_recording_samples {
                            samples.truncate(shared.max_recording_samples);
                            send(ws, &ServerMessage::error(ErrorCode::TooLong)).await?;
                            phase = transcribe(shared, ws, phase).await?;
                        }
                    }
                    Incoming::Control(ClientMessage::Ping) => send(ws, &ServerMessage::Pong).await?,
                    Incoming::Control(ClientMessage::Start { output, post_process }) => {
                        match phase {
                            // Starting again starts over.
                            Phase::Recording { slot, .. } => {
                                phase = Phase::Recording {
                                    slot,
                                    samples: Vec::new(),
                                    output,
                                    post_process,
                                };
                            }
                            Phase::Transcribing { .. } => {
                                send(ws, &ServerMessage::error(ErrorCode::Busy)).await?;
                                continue;
                            }
                            Phase::Idle => match Slot::claim(shared, device_id) {
                                Ok(slot) => {
                                    shared.host.state_changed(device_id, SessionState::Recording);
                                    phase = Phase::Recording {
                                        slot,
                                        samples: Vec::new(),
                                        output,
                                        post_process,
                                    };
                                }
                                Err(code) => {
                                    send(ws, &ServerMessage::error(code)).await?;
                                    continue;
                                }
                            },
                        }
                        send(ws, &ServerMessage::State { state: SessionState::Recording }).await?;
                    }
                    Incoming::Control(ClientMessage::Stop) => {
                        if matches!(phase, Phase::Recording { .. }) {
                            phase = transcribe(shared, ws, phase).await?;
                        }
                    }
                    Incoming::Control(ClientMessage::Cancel) => match &phase {
                        Phase::Recording { .. } => {
                            // Dropping the slot ends the session.
                            phase = Phase::Idle;
                            send(ws, &ServerMessage::State { state: SessionState::Idle }).await?;
                        }
                        // The slot stays taken until the engine lets go; the
                        // phone hears `idle` then.
                        Phase::Transcribing { cancelled, .. } => {
                            cancelled.store(true, Ordering::Release);
                        }
                        Phase::Idle => {}
                    },
                    Incoming::Control(ClientMessage::Pair { .. } | ClientMessage::Auth { .. }) => {
                        send(ws, &ServerMessage::error(ErrorCode::BadRequest)).await?;
                    }
                }
            }
        }
    }
}

/// Hands a finished recording to the host on a blocking thread. The task
/// owns the slot, so the session ends when the transcription does even if
/// the phone has gone by then (a paste on the desktop still lands).
async fn transcribe<S>(shared: &Arc<Shared>, ws: &mut Ws<S>, phase: Phase) -> Result<Phase, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let Phase::Recording {
        slot,
        samples,
        output,
        post_process,
    } = phase
    else {
        return Ok(phase);
    };
    let cancelled = Arc::new(AtomicBool::new(false));
    let (tx, done) = oneshot::channel();
    let recording = Recording {
        device_id: slot.device_id.clone(),
        samples,
        output,
        post_process,
        cancelled: Arc::clone(&cancelled),
    };
    shared
        .host
        .state_changed(&slot.device_id, SessionState::Transcribing);
    let host = Arc::clone(&shared.host);
    tokio::spawn(async move {
        let result = tokio::task::spawn_blocking(move || host.transcribe(recording))
            .await
            .unwrap_or_else(|e| {
                Err(Failure::new(ErrorCode::TranscriptionFailed)
                    .with_message(format!("transcription crashed: {e}")))
            });
        drop(slot);
        let _ = tx.send(result);
    });
    send(
        ws,
        &ServerMessage::State {
            state: SessionState::Transcribing,
        },
    )
    .await?;
    Ok(Phase::Transcribing { cancelled, done })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_names_are_cleaned() {
        assert_eq!(clean_device_name("  Pixel 9\n"), "Pixel 9");
        assert_eq!(clean_device_name("\u{7}\t"), "Phone");
        assert_eq!(clean_device_name(&"x".repeat(100)).len(), 40);
    }
}

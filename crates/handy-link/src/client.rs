//! The phone side of the link: pair once, then connect and stream.

use crate::pairing::PairingPayload;
use crate::protocol::{
    ClientMessage, ErrorCode, ModelInfo, OutputMode, ServerMessage, PROTOCOL_VERSION,
};
use crate::tls::pinned_client_config;
use futures_util::{SinkExt, StreamExt};
use rustls::pki_types::ServerName;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

/// Per address. A desktop on the same network answers in milliseconds; an
/// address it no longer has should give up quickly so the next gets a turn.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// How often the link says it's still there. Well inside the server's idle
/// limit, and often enough that a phone notices a dead link (Wi-Fi gone, the
/// desktop asleep) before its user has finished a sentence into it.
pub const PING_INTERVAL: Duration = Duration::from_secs(5);
/// Silence for this long means the desktop is gone, whatever TCP still
/// believes. Every ping is answered, so this is two missed answers.
const DEAD_AFTER: Duration = Duration::from_secs(12);

/// What a paired phone remembers about its desktop. The secret makes this
/// worth protecting: keep it in app-private storage.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Desktop {
    pub desktop_id: String,
    pub desktop_name: String,
    /// Where it was last reached, likeliest first.
    pub addresses: Vec<IpAddr>,
    pub port: u16,
    pub fingerprint: String,
    pub device_id: String,
    pub secret: String,
}

#[derive(Debug)]
pub enum LinkError {
    /// Nothing answered at any known address.
    Unreachable(String),
    /// Something answered, but it isn't the paired desktop (wrong
    /// certificate), or it doesn't speak the protocol.
    Handshake(String),
    /// The desktop said no.
    Refused(ErrorCode),
    /// The connection dropped.
    Closed,
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreachable(why) => write!(f, "the desktop can't be reached: {why}"),
            Self::Handshake(why) => write!(f, "couldn't set up a secure connection: {why}"),
            Self::Refused(code) => write!(f, "the desktop refused: {code:?}"),
            Self::Closed => f.write_str("the connection closed"),
        }
    }
}

impl std::error::Error for LinkError {}

type Ws = WebSocketStream<TlsStream<TcpStream>>;

/// Tries each address in turn and returns the first that turns out to be the
/// desktop with this fingerprint.
async fn open(
    addresses: &[IpAddr],
    port: u16,
    fingerprint: &str,
) -> Result<(Ws, IpAddr), LinkError> {
    let connector =
        TlsConnector::from(pinned_client_config(fingerprint).map_err(LinkError::Handshake)?);
    // The pin identifies the desktop; this name is never checked.
    let name = ServerName::try_from("handy.local").expect("a valid DNS name");
    let mut last = LinkError::Unreachable("no address is known".into());
    for &ip in addresses {
        let addr = SocketAddr::new(ip, port);
        let tcp = match timeout(CONNECT_TIMEOUT, TcpStream::connect(addr)).await {
            Ok(Ok(tcp)) => tcp,
            Ok(Err(e)) => {
                last = LinkError::Unreachable(format!("{addr}: {e}"));
                continue;
            }
            Err(_) => {
                last = LinkError::Unreachable(format!("{addr}: timed out"));
                continue;
            }
        };
        let _ = tcp.set_nodelay(true);
        let tls = match timeout(HANDSHAKE_TIMEOUT, connector.connect(name.clone(), tcp)).await {
            Ok(Ok(tls)) => tls,
            Ok(Err(e)) => {
                last = LinkError::Handshake(format!("{addr}: {e}"));
                continue;
            }
            Err(_) => {
                last = LinkError::Handshake(format!("{addr}: timed out"));
                continue;
            }
        };
        let url = format!("wss://handy.local:{port}/");
        match timeout(HANDSHAKE_TIMEOUT, tokio_tungstenite::client_async(url, tls)).await {
            Ok(Ok((ws, _))) => return Ok((ws, ip)),
            Ok(Err(e)) => last = LinkError::Handshake(format!("{addr}: {e}")),
            Err(_) => last = LinkError::Handshake(format!("{addr}: timed out")),
        }
    }
    Err(last)
}

async fn send_control(ws: &mut Ws, message: &ClientMessage) -> Result<(), LinkError> {
    let json = serde_json::to_string(message).expect("messages serialize");
    ws.send(Message::text(json))
        .await
        .map_err(|_| LinkError::Closed)
}

async fn next_control(ws: &mut Ws) -> Result<ServerMessage, LinkError> {
    loop {
        match ws.next().await {
            Some(Ok(Message::Text(text))) => {
                // What a newer desktop says that this version doesn't know.
                if let Ok(message) = serde_json::from_str(text.as_str()) {
                    return Ok(message);
                }
            }
            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return Err(LinkError::Closed),
            Some(Ok(_)) => {}
        }
    }
}

/// What the desktop said about itself when the link came up.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopInfo {
    pub name: String,
    pub version: String,
    pub default_output: OutputMode,
    /// Empty from a desktop that predates them; see
    /// [`crate::protocol::feature`].
    pub features: Vec<String>,
    pub model: Option<ModelInfo>,
    /// Whether the desktop lets this device paste into it. `None` from a
    /// desktop that doesn't say.
    #[serde(default)]
    pub can_type: Option<bool>,
}

impl DesktopInfo {
    pub fn has(&self, feature: &str) -> bool {
        self.features.iter().any(|f| f == feature)
    }
}

/// Who is connecting, beyond its pairing.
#[derive(Clone, Debug, Default)]
pub struct ClientInfo {
    /// See [`crate::protocol::device_kind`]. `None` reads as a phone.
    pub device_kind: Option<String>,
    pub features: Vec<String>,
}

/// How [`connect_with`] looks for the desktop.
#[derive(Clone, Debug)]
pub struct ConnectOptions {
    pub client: ClientInfo,
    /// Broadcast on the network when no known address answers. Off for a
    /// reconnect loop that would otherwise shout every few seconds.
    pub discover: bool,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        Self {
            client: ClientInfo::default(),
            discover: true,
        }
    }
}

/// Puts the address that worked first, so the next connect tries it first.
fn prefer(addresses: &mut Vec<IpAddr>, ip: IpAddr) {
    addresses.retain(|a| *a != ip);
    addresses.insert(0, ip);
}

/// Pairs with the desktop a scanned code describes. The link is up and
/// authenticated when this returns; keep the [`Desktop`] to reconnect later.
pub async fn pair(
    payload: &PairingPayload,
    device_name: &str,
) -> Result<(Desktop, DesktopInfo, Link), LinkError> {
    pair_with(payload, device_name, &ClientInfo::default()).await
}

/// [`pair`], saying what kind of device this is and what it can do.
pub async fn pair_with(
    payload: &PairingPayload,
    device_name: &str,
    client: &ClientInfo,
) -> Result<(Desktop, DesktopInfo, Link), LinkError> {
    let (mut ws, ip) = open(&payload.addresses, payload.port, &payload.fingerprint).await?;
    send_control(
        &mut ws,
        &ClientMessage::Pair {
            protocol: PROTOCOL_VERSION,
            token: payload.token.clone(),
            device_name: device_name.to_string(),
            device_kind: client.device_kind.clone(),
            features: client.features.clone(),
        },
    )
    .await?;
    match timeout(HANDSHAKE_TIMEOUT, next_control(&mut ws))
        .await
        .map_err(|_| LinkError::Handshake("the desktop didn't answer".into()))??
    {
        ServerMessage::Paired {
            device_id,
            secret,
            desktop_name,
            desktop_version,
            default_output,
            features,
            model,
            can_type,
        } => {
            let mut addresses = payload.addresses.clone();
            prefer(&mut addresses, ip);
            let desktop = Desktop {
                desktop_id: payload.desktop_id.clone(),
                desktop_name: desktop_name.clone(),
                addresses,
                port: payload.port,
                fingerprint: payload.fingerprint.clone(),
                device_id,
                secret,
            };
            let info = DesktopInfo {
                name: desktop_name,
                version: desktop_version,
                default_output,
                features,
                model,
                can_type,
            };
            Ok((desktop, info, Link::start(ws)))
        }
        ServerMessage::Error { code, .. } => Err(LinkError::Refused(code)),
        _ => Err(LinkError::Handshake("unexpected answer to pairing".into())),
    }
}

/// Connects to a paired desktop. If none of its known addresses answer, asks
/// the network where it went. On success `desktop` holds where it was found,
/// and the name it goes by now.
///
/// `channel` names the part of the app connecting. The desktop keeps one
/// connection per channel: connecting again on a channel replaces what was
/// there, so parts that connect on their own each need their own.
pub async fn connect(
    desktop: &mut Desktop,
    channel: &str,
) -> Result<(DesktopInfo, Link), LinkError> {
    connect_with(desktop, channel, &ConnectOptions::default()).await
}

/// [`connect`], with control over how the desktop is looked for and what
/// this client tells it about itself.
pub async fn connect_with(
    desktop: &mut Desktop,
    channel: &str,
    options: &ConnectOptions,
) -> Result<(DesktopInfo, Link), LinkError> {
    let (mut ws, ip) = match open(&desktop.addresses, desktop.port, &desktop.fingerprint).await {
        Ok(opened) => opened,
        Err(first) if !options.discover => return Err(first),
        // A wrong certificate at an old address usually means that address
        // now belongs to something else, so look for the desktop either way.
        Err(first) => {
            let found = crate::discovery::find(
                &desktop.desktop_id,
                desktop.port,
                &desktop.addresses,
                Duration::from_millis(1500),
            )
            .await;
            match found {
                Some(addr) => open(&[addr.ip()], addr.port(), &desktop.fingerprint).await?,
                None => return Err(first),
            }
        }
    };
    send_control(
        &mut ws,
        &ClientMessage::Auth {
            protocol: PROTOCOL_VERSION,
            device_id: desktop.device_id.clone(),
            secret: desktop.secret.clone(),
            channel: channel.to_string(),
            features: options.client.features.clone(),
        },
    )
    .await?;
    match timeout(HANDSHAKE_TIMEOUT, next_control(&mut ws))
        .await
        .map_err(|_| LinkError::Handshake("the desktop didn't answer".into()))??
    {
        ServerMessage::Ready {
            desktop_name,
            desktop_version,
            default_output,
            features,
            model,
            can_type,
        } => {
            prefer(&mut desktop.addresses, ip);
            desktop.desktop_name = desktop_name.clone();
            let info = DesktopInfo {
                name: desktop_name,
                version: desktop_version,
                default_output,
                features,
                model,
                can_type,
            };
            Ok((info, Link::start(ws)))
        }
        ServerMessage::Error { code, .. } => Err(LinkError::Refused(code)),
        _ => Err(LinkError::Handshake("unexpected answer to login".into())),
    }
}

/// Asks a paired desktop to forget this device, for when the device forgets
/// the desktop. Best effort: a desktop that can't be reached, or is too old
/// to know the message, keeps its entry until it's removed there.
pub async fn unpair(desktop: &mut Desktop, client: &ClientInfo) -> Result<(), LinkError> {
    let options = ConnectOptions {
        client: client.clone(),
        discover: true,
    };
    // A channel of its own, so no connection in use is replaced by this one.
    let (_, link) = connect_with(desktop, "unpair", &options).await?;
    link.send(ClientMessage::Unpair)?;
    // The desktop closes once it has done it.
    let mut link = link;
    let _ = timeout(Duration::from_secs(3), async {
        while link.recv().await.is_some() {}
    })
    .await;
    Ok(())
}

enum Outgoing {
    Control(ClientMessage),
    Audio(Vec<u8>),
}

/// An authenticated connection. Sending never blocks; a background task
/// writes, pings while idle, and delivers what the desktop says to
/// [`Link::recv`]. Dropping the link closes it.
pub struct Link {
    sender: LinkSender,
    incoming: mpsc::UnboundedReceiver<ServerMessage>,
}

/// The sending half, cheap to clone into wherever audio comes from.
#[derive(Clone)]
pub struct LinkSender {
    outgoing: mpsc::UnboundedSender<Outgoing>,
}

impl LinkSender {
    pub fn send(&self, message: ClientMessage) -> Result<(), LinkError> {
        self.outgoing
            .send(Outgoing::Control(message))
            .map_err(|_| LinkError::Closed)
    }

    /// 16 kHz mono signed 16-bit little-endian PCM.
    pub fn send_audio(&self, pcm: Vec<u8>) -> Result<(), LinkError> {
        self.outgoing
            .send(Outgoing::Audio(pcm))
            .map_err(|_| LinkError::Closed)
    }
}

impl Link {
    fn start(ws: Ws) -> Self {
        let (outgoing, mut to_send) = mpsc::unbounded_channel();
        let (deliver, incoming) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let (mut sink, mut stream) = ws.split();
            let mut ping = tokio::time::interval(PING_INTERVAL);
            ping.tick().await;
            let mut last_heard = tokio::time::Instant::now();
            loop {
                tokio::select! {
                    outgoing = to_send.recv() => {
                        let message = match outgoing {
                            Some(Outgoing::Control(message)) => {
                                Message::text(serde_json::to_string(&message).expect("messages serialize"))
                            }
                            Some(Outgoing::Audio(pcm)) => Message::binary(pcm),
                            // Every sender is gone: the link was dropped.
                            None => break,
                        };
                        if sink.send(message).await.is_err() {
                            break;
                        }
                    }
                    _ = ping.tick() => {
                        if last_heard.elapsed() > DEAD_AFTER {
                            break;
                        }
                        let json = serde_json::to_string(&ClientMessage::Ping).expect("messages serialize");
                        if sink.send(Message::text(json)).await.is_err() {
                            break;
                        }
                    }
                    // Nobody is listening any more: the link was dropped.
                    _ = deliver.closed() => break,
                    message = stream.next() => match message {
                        Some(Ok(Message::Text(text))) => {
                            last_heard = tokio::time::Instant::now();
                            match serde_json::from_str::<ServerMessage>(text.as_str()) {
                                Ok(ServerMessage::Pong) | Err(_) => {}
                                Ok(message) => {
                                    if deliver.send(message).is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                        Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                        Some(Ok(_)) => {}
                    },
                }
            }
            let _ = sink.close().await;
            // Dropping `deliver` here is how `recv` learns the link is down.
        });
        Self {
            sender: LinkSender { outgoing },
            incoming,
        }
    }

    pub fn sender(&self) -> LinkSender {
        self.sender.clone()
    }

    pub fn send(&self, message: ClientMessage) -> Result<(), LinkError> {
        self.sender.send(message)
    }

    pub fn send_audio(&self, pcm: Vec<u8>) -> Result<(), LinkError> {
        self.sender.send_audio(pcm)
    }

    /// The next thing the desktop says. `None` once the link is down.
    pub async fn recv(&mut self) -> Option<ServerMessage> {
        self.incoming.recv().await
    }
}

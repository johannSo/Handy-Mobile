//! The whole exchange over a real socket: pair, reconnect, record, cancel,
//! and everything the server must refuse.

use handy_link::client::{self, Desktop, Link, LinkError};
use handy_link::pairing::{hash_secret, new_secret, random_hex, secret_matches, PairingPayload};
use handy_link::protocol::{
    f32_to_pcm16, ClientMessage, ErrorCode, OutputMode, ServerMessage, SessionState,
};
use handy_link::server::{
    Failure, Host, HostInfo, PairedDevice, Recording, Server, ServerOptions, Transcript,
};
use handy_link::Identity;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Default)]
struct TestHost {
    token: Mutex<Option<String>>,
    /// Device id to secret hash.
    devices: Mutex<HashMap<String, String>>,
    states: Mutex<Vec<SessionState>>,
    /// Holds `transcribe` until released, to test what happens meanwhile.
    hold: AtomicBool,
    no_model: AtomicBool,
    saw_cancel: AtomicBool,
}

impl Host for TestHost {
    fn info(&self) -> HostInfo {
        HostInfo {
            name: "Test Desktop".into(),
            version: "1.2.3".into(),
            default_output: OutputMode::Desktop,
        }
    }

    fn pair(&self, token: &str, _device_name: &str) -> Option<PairedDevice> {
        let mut expected = self.token.lock().unwrap();
        if expected.as_deref() != Some(token) {
            return None;
        }
        *expected = None;
        let device_id = random_hex(8);
        let secret = new_secret();
        self.devices
            .lock()
            .unwrap()
            .insert(device_id.clone(), hash_secret(&secret));
        Some(PairedDevice { device_id, secret })
    }

    fn authenticate(&self, device_id: &str, secret: &str) -> bool {
        self.devices
            .lock()
            .unwrap()
            .get(device_id)
            .is_some_and(|hash| secret_matches(secret, hash))
    }

    fn begin(&self, _device_id: &str) -> Result<(), ErrorCode> {
        if self.no_model.load(Ordering::Acquire) {
            Err(ErrorCode::NoModel)
        } else {
            Ok(())
        }
    }

    fn state_changed(&self, _device_id: &str, state: SessionState) {
        self.states.lock().unwrap().push(state);
    }

    fn transcribe(&self, recording: Recording) -> Result<Transcript, Failure> {
        while self.hold.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(5));
        }
        if recording.cancelled.load(Ordering::Acquire) {
            self.saw_cancel.store(true, Ordering::Release);
        }
        let pasted = recording.output.unwrap_or(OutputMode::Desktop) == OutputMode::Desktop;
        Ok(Transcript {
            text: format!("{} samples", recording.samples.len()),
            pasted,
        })
    }
}

struct Fixture {
    host: Arc<TestHost>,
    server: Server,
    identity: Identity,
}

impl Fixture {
    async fn start(options: ServerOptions) -> Self {
        let host = Arc::new(TestHost::default());
        let identity = Identity::generate().unwrap();
        let server = Server::bind(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            &identity,
            host.clone(),
            options,
        )
        .await
        .unwrap();
        Self {
            host,
            server,
            identity,
        }
    }

    fn payload(&self, token: &str) -> PairingPayload {
        *self.host.token.lock().unwrap() = Some(token.to_string());
        PairingPayload {
            desktop_id: "desk".into(),
            desktop_name: "Test Desktop".into(),
            addresses: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
            port: self.server.local_addr().port(),
            fingerprint: self.identity.fingerprint(),
            token: token.into(),
        }
    }

    async fn paired(&self) -> (Desktop, Link) {
        let (desktop, _, link) = client::pair(&self.payload("tok"), "Pixel").await.unwrap();
        (desktop, link)
    }
}

async fn next(link: &mut Link) -> ServerMessage {
    tokio::time::timeout(Duration::from_secs(5), link.recv())
        .await
        .expect("the desktop answers in time")
        .expect("the link stays up")
}

fn audio(samples: usize) -> Vec<u8> {
    f32_to_pcm16(&vec![0.25; samples])
}

fn state(state: SessionState) -> ServerMessage {
    ServerMessage::State { state }
}

#[tokio::test(flavor = "multi_thread")]
async fn pairs_then_reconnects_and_transcribes() {
    let fx = Fixture::start(ServerOptions::default()).await;
    let (mut desktop, link) = fx.paired().await;
    assert_eq!(desktop.desktop_name, "Test Desktop");
    assert_eq!(
        fx.server.connected_devices(),
        vec![desktop.device_id.clone()]
    );
    drop(link);

    let (info, mut link) = client::connect(&mut desktop, "app").await.unwrap();
    assert_eq!(info.version, "1.2.3");
    assert_eq!(info.default_output, OutputMode::Desktop);

    link.send(ClientMessage::Start {
        output: Some(OutputMode::Phone),
        post_process: None,
    })
    .unwrap();
    assert_eq!(next(&mut link).await, state(SessionState::Recording));
    for _ in 0..10 {
        link.send_audio(audio(1600)).unwrap();
    }
    link.send(ClientMessage::Stop).unwrap();
    assert_eq!(next(&mut link).await, state(SessionState::Transcribing));
    assert_eq!(
        next(&mut link).await,
        ServerMessage::Final {
            text: "16000 samples".into(),
            pasted: false
        }
    );
    assert_eq!(next(&mut link).await, state(SessionState::Idle));
    assert_eq!(
        *fx.host.states.lock().unwrap(),
        vec![
            SessionState::Recording,
            SessionState::Transcribing,
            SessionState::Idle
        ]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pairing_token_works_once() {
    let fx = Fixture::start(ServerOptions::default()).await;
    let payload = fx.payload("once");
    client::pair(&payload, "First").await.unwrap();
    let again = client::pair(&payload, "Second").await;
    assert!(matches!(
        again,
        Err(LinkError::Refused(ErrorCode::InvalidToken))
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn wrong_secrets_are_refused_then_rate_limited() {
    let fx = Fixture::start(ServerOptions::default()).await;
    let (desktop, _link) = fx.paired().await;
    let mut intruder = Desktop {
        secret: new_secret(),
        ..desktop.clone()
    };
    for _ in 0..5 {
        let refused = client::connect(&mut intruder, "app").await;
        assert!(matches!(
            refused,
            Err(LinkError::Refused(ErrorCode::Unauthorized))
        ));
    }
    // From now on the address has to wait, right secret or not.
    let mut desktop = desktop;
    let limited = client::connect(&mut desktop, "app").await;
    assert!(matches!(
        limited,
        Err(LinkError::Refused(ErrorCode::RateLimited))
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_different_certificate_is_not_the_desktop() {
    let fx = Fixture::start(ServerOptions::default()).await;
    let mut payload = fx.payload("tok");
    payload.fingerprint = Identity::generate().unwrap().fingerprint();
    let result = client::pair(&payload, "Pixel").await;
    assert!(
        matches!(result, Err(LinkError::Handshake(_))),
        "{:?}",
        result.err()
    );
    // The token wasn't spent on the impostor check.
    assert!(fx.host.token.lock().unwrap().is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_drops_a_recording_and_a_result() {
    let fx = Fixture::start(ServerOptions::default()).await;
    let (_desktop, mut link) = fx.paired().await;

    // While recording: nothing is transcribed.
    link.send(ClientMessage::Start {
        output: None,
        post_process: None,
    })
    .unwrap();
    assert_eq!(next(&mut link).await, state(SessionState::Recording));
    link.send_audio(audio(1600)).unwrap();
    link.send(ClientMessage::Cancel).unwrap();
    assert_eq!(next(&mut link).await, state(SessionState::Idle));

    // While transcribing: the host is told, and no text comes back.
    fx.host.hold.store(true, Ordering::Release);
    link.send(ClientMessage::Start {
        output: None,
        post_process: None,
    })
    .unwrap();
    assert_eq!(next(&mut link).await, state(SessionState::Recording));
    link.send(ClientMessage::Stop).unwrap();
    assert_eq!(next(&mut link).await, state(SessionState::Transcribing));
    link.send(ClientMessage::Cancel).unwrap();
    // Messages are handled in order, so this answer means the cancel was too.
    link.send(ClientMessage::Start {
        output: None,
        post_process: None,
    })
    .unwrap();
    assert_eq!(next(&mut link).await, ServerMessage::error(ErrorCode::Busy));
    fx.host.hold.store(false, Ordering::Release);
    assert_eq!(next(&mut link).await, state(SessionState::Idle));
    assert!(fx.host.saw_cancel.load(Ordering::Acquire));
}

#[tokio::test(flavor = "multi_thread")]
async fn one_phone_at_a_time() {
    let fx = Fixture::start(ServerOptions::default()).await;
    let (_a, mut first) = fx.paired().await;
    let (_b, _, mut second) = client::pair(&fx.payload("tok2"), "Other").await.unwrap();

    first
        .send(ClientMessage::Start {
            output: None,
            post_process: None,
        })
        .unwrap();
    assert_eq!(next(&mut first).await, state(SessionState::Recording));
    second
        .send(ClientMessage::Start {
            output: None,
            post_process: None,
        })
        .unwrap();
    assert_eq!(
        next(&mut second).await,
        ServerMessage::error(ErrorCode::Busy)
    );

    // A phone that vanishes mid-recording frees the desktop.
    drop(first);
    let mut freed = false;
    for _ in 0..100 {
        second
            .send(ClientMessage::Start {
                output: None,
                post_process: None,
            })
            .unwrap();
        match next(&mut second).await {
            ServerMessage::State {
                state: SessionState::Recording,
            } => {
                freed = true;
                break;
            }
            _ => tokio::time::sleep(Duration::from_millis(20)).await,
        }
    }
    assert!(freed);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_host_can_refuse_a_recording() {
    let fx = Fixture::start(ServerOptions::default()).await;
    let (_desktop, mut link) = fx.paired().await;
    fx.host.no_model.store(true, Ordering::Release);
    link.send(ClientMessage::Start {
        output: None,
        post_process: None,
    })
    .unwrap();
    assert_eq!(
        next(&mut link).await,
        ServerMessage::error(ErrorCode::NoModel)
    );
    assert!(fx.host.states.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_recording_that_runs_too_long_is_cut_and_transcribed() {
    let fx = Fixture::start(ServerOptions {
        max_recording_samples: 4000,
        ..ServerOptions::default()
    })
    .await;
    let (_desktop, mut link) = fx.paired().await;
    link.send(ClientMessage::Start {
        output: None,
        post_process: None,
    })
    .unwrap();
    assert_eq!(next(&mut link).await, state(SessionState::Recording));
    for _ in 0..5 {
        link.send_audio(audio(1600)).unwrap();
    }
    assert_eq!(
        next(&mut link).await,
        ServerMessage::error(ErrorCode::TooLong)
    );
    assert_eq!(next(&mut link).await, state(SessionState::Transcribing));
    assert_eq!(
        next(&mut link).await,
        ServerMessage::Final {
            text: "4000 samples".into(),
            pasted: true
        }
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_connection_replaces_only_its_own_channel() {
    let fx = Fixture::start(ServerOptions::default()).await;
    let (mut desktop, link) = fx.paired().await;
    drop(link);
    let (_, mut app) = client::connect(&mut desktop, "app").await.unwrap();
    let (_, mut keyboard) = client::connect(&mut desktop, "keyboard").await.unwrap();

    // The keyboard connecting left the app's connection alone.
    app.send(ClientMessage::Start {
        output: None,
        post_process: None,
    })
    .unwrap();
    assert_eq!(next(&mut app).await, state(SessionState::Recording));

    // The app connecting again replaces that one, and only that one.
    let (_, _app_again) = client::connect(&mut desktop, "app").await.unwrap();
    let closed = tokio::time::timeout(Duration::from_secs(5), app.recv()).await;
    assert_eq!(closed, Ok(None));
    keyboard
        .send(ClientMessage::Start {
            output: None,
            post_process: None,
        })
        .unwrap();
    assert_eq!(next(&mut keyboard).await, state(SessionState::Recording));
}

#[tokio::test(flavor = "multi_thread")]
async fn unpairing_disconnects_the_phone() {
    let fx = Fixture::start(ServerOptions::default()).await;
    let (mut desktop, mut link) = fx.paired().await;
    fx.host.devices.lock().unwrap().clear();
    fx.server.disconnect(&desktop.device_id);
    let closed = tokio::time::timeout(Duration::from_secs(5), link.recv()).await;
    assert_eq!(closed, Ok(None));
    let refused = client::connect(&mut desktop, "app").await;
    assert!(matches!(
        refused,
        Err(LinkError::Refused(ErrorCode::Unauthorized))
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_moved_desktop_is_found_on_the_network() {
    let fx = Fixture::start(ServerOptions {
        discoverable_as: Some("desk".into()),
        ..ServerOptions::default()
    })
    .await;
    let (mut desktop, link) = fx.paired().await;
    drop(link);
    // The phone remembers an address nothing answers at any more.
    desktop.addresses = vec!["127.0.0.2".parse().unwrap()];
    match client::connect(&mut desktop, "app").await {
        Ok(_) => assert_ne!(desktop.addresses[0], "127.0.0.2".parse::<IpAddr>().unwrap()),
        // No broadcast route (a sandbox with only loopback): nothing to find.
        Err(LinkError::Unreachable(_)) if handy_link::discovery::local_ipv4().is_none() => {}
        Err(e) => panic!("the desktop wasn't found: {e}"),
    }
}

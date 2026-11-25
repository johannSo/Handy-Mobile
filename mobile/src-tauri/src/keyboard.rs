//! The links behind the Handy keyboard (`HandyKeyboardService.kt`) and the
//! floating bubble (`HandyBubbleService.kt`).
//!
//! Android runs these as services, often in a process where the app's
//! window, and so Tauri, never started. So they don't share the app's link:
//! each has its own runtime here and its own connection (on its own channel,
//! so none replaces another), up only while it is on screen. Kotlin records
//! the microphone, calls these functions through `PhoneLink.kt`, and waits on
//! [`next_event`] for what comes back.

use crate::{read_desktop, write_desktop, RETRY_DELAYS};
use handy_link::client::{self, LinkError, LinkSender};
use handy_link::protocol::{ClientMessage, ErrorCode, OutputMode, ServerMessage, SessionState};
use jni::objects::{JByteArray, JClass, JString};
use jni::sys::{jint, jstring};
use jni::JNIEnv;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Mutex, OnceLock, PoisonError};
use tokio::runtime::Runtime;
use tokio::sync::{watch, Notify};

/// What the keyboard is told, as JSON. Kotlin reads `type` first.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Event {
    Link {
        state: LinkState,
        /// The computer's name, once it's known.
        name: Option<String>,
    },
    Session {
        state: SessionState,
    },
    Final {
        text: String,
    },
    Error {
        code: ErrorCode,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum LinkState {
    Unpaired,
    Connecting,
    Connected,
    Offline,
    Rejected,
    Incompatible,
}

struct Keyboard {
    channel: &'static str,
    runtime: Runtime,
    /// Counts openings. A connection task stops when this moves on.
    opened: watch::Sender<u64>,
    sender: Mutex<Option<LinkSender>>,
    events: Sender<Event>,
    waiting: Mutex<Receiver<Event>>,
    retry_now: Notify,
}

/// The channel each link connects on. `PhoneLink.kt` numbers them the same.
const CHANNELS: [&str; 2] = ["keyboard", "bubble"];

fn keyboard(id: jint) -> Option<&'static Keyboard> {
    static LINKS: [OnceLock<Option<Keyboard>>; CHANNELS.len()] = [OnceLock::new(), OnceLock::new()];
    let index = usize::try_from(id).ok()?;
    LINKS
        .get(index)?
        .get_or_init(|| {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .thread_name("handy-keyboard")
                .enable_all()
                .build()
                .ok()?;
            let (events, waiting) = channel();
            Some(Keyboard {
                channel: CHANNELS[index],
                runtime,
                opened: watch::channel(0).0,
                sender: Mutex::new(None),
                events,
                waiting: Mutex::new(waiting),
                retry_now: Notify::new(),
            })
        })
        .as_ref()
}

fn set_sender(keyboard: &Keyboard, sender: Option<LinkSender>) {
    *keyboard
        .sender
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = sender;
}

fn send(id: jint, message: ClientMessage) {
    let Some(keyboard) = keyboard(id) else { return };
    let sender = keyboard
        .sender
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    if let Some(sender) = sender {
        let _ = sender.send(message);
    }
}

/// Connects to the desktop paired in `path` and stays connected, through
/// drops, until [`close`] or the next `open`.
fn open(id: jint, path: PathBuf) {
    let Some(keyboard) = keyboard(id) else { return };
    set_sender(keyboard, None);
    keyboard.opened.send_modify(|n| *n += 1);
    let mut moved_on = keyboard.opened.subscribe();
    let emit = |event| {
        let _ = keyboard.events.send(event);
    };

    keyboard.runtime.spawn(async move {
        let link = |state, name: &Option<String>| Event::Link {
            state,
            name: name.clone(),
        };
        let mut attempt = 0usize;
        loop {
            // Read every time: the app may have paired again since.
            let Some(mut desktop) = read_desktop(&path) else {
                emit(link(LinkState::Unpaired, &None));
                return;
            };
            let name = Some(desktop.desktop_name.clone());
            let known = desktop.clone();
            if attempt == 0 {
                emit(link(LinkState::Connecting, &name));
            }

            let connected = tokio::select! {
                connected = client::connect(&mut desktop, keyboard.channel) => connected,
                _ = moved_on.changed() => return,
            };
            match connected {
                Ok((info, mut link_up)) => {
                    attempt = 0;
                    // Found at a new address: remember it, unless the app
                    // paired with something else in the meantime.
                    if desktop != known && read_desktop(&path).as_ref() == Some(&known) {
                        let _ = write_desktop(
                            &path,
                            &desktop,
                            &format!("json.{}.tmp", keyboard.channel),
                        );
                    }
                    set_sender(keyboard, Some(link_up.sender()));
                    let name = Some(info.name);
                    emit(link(LinkState::Connected, &name));
                    loop {
                        let message = tokio::select! {
                            message = link_up.recv() => message,
                            _ = moved_on.changed() => return,
                        };
                        match message {
                            Some(ServerMessage::State { state }) => emit(Event::Session { state }),
                            Some(ServerMessage::Final { text, .. }) => emit(Event::Final { text }),
                            Some(ServerMessage::Error { code, text, .. }) => {
                                // The transcript still came; the keyboard
                                // never asked the desktop to paste it.
                                match text {
                                    Some(text) => emit(Event::Final { text }),
                                    None => emit(Event::Error { code }),
                                }
                            }
                            Some(_) => {}
                            None => break,
                        }
                    }
                    set_sender(keyboard, None);
                    emit(link(LinkState::Offline, &name));
                }
                Err(LinkError::Refused(ErrorCode::Unauthorized)) => {
                    emit(link(LinkState::Rejected, &name));
                    return;
                }
                Err(LinkError::Refused(ErrorCode::UnsupportedProtocol)) => {
                    emit(link(LinkState::Incompatible, &name));
                    return;
                }
                Err(_) => emit(link(LinkState::Offline, &name)),
            }

            let delay = RETRY_DELAYS[attempt.min(RETRY_DELAYS.len() - 1)];
            attempt += 1;
            tokio::select! {
                _ = tokio::time::sleep(delay) => {}
                _ = keyboard.retry_now.notified() => {}
                _ = moved_on.changed() => return,
            }
        }
    });
}

/// Drops the link. Whatever was being recorded or transcribed is lost.
fn close(id: jint) {
    let Some(keyboard) = keyboard(id) else { return };
    set_sender(keyboard, None);
    keyboard.opened.send_modify(|n| *n += 1);
}

/// Blocks until the link has something to say.
fn next_event(id: jint) -> Option<String> {
    let keyboard = keyboard(id)?;
    let event = keyboard
        .waiting
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .recv()
        .ok()?;
    serde_json::to_string(&event).ok()
}

// Called from `PhoneLink.kt`, with the number of the link meant. None of these may panic: unwinding into the
// Java runtime takes the whole app down.

#[no_mangle]
pub extern "system" fn Java_com_pais_handy_mobile_PhoneLink_nativeOpen<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    id: jint,
    path: JString<'local>,
) {
    if let Ok(path) = env.get_string(&path) {
        open(id, PathBuf::from(String::from(path)));
    }
}

#[no_mangle]
pub extern "system" fn Java_com_pais_handy_mobile_PhoneLink_nativeClose<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    id: jint,
) {
    close(id);
}

#[no_mangle]
pub extern "system" fn Java_com_pais_handy_mobile_PhoneLink_nativeRetry<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    id: jint,
) {
    if let Some(keyboard) = keyboard(id) {
        keyboard.retry_now.notify_waiters();
    }
}

/// Returns null only if the link can never work, which ends Kotlin's loop.
#[no_mangle]
pub extern "system" fn Java_com_pais_handy_mobile_PhoneLink_nativeNextEvent<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
    id: jint,
) -> jstring {
    next_event(id)
        .and_then(|json| env.new_string(json).ok())
        .map_or(std::ptr::null_mut(), |string| string.into_raw())
}

#[no_mangle]
pub extern "system" fn Java_com_pais_handy_mobile_PhoneLink_nativeStart<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    id: jint,
) {
    // The text is typed on the phone, so the desktop must not paste it too.
    send(
        id,
        ClientMessage::Start {
            output: Some(OutputMode::Phone),
            post_process: None,
            options: None,
        },
    );
}

#[no_mangle]
pub extern "system" fn Java_com_pais_handy_mobile_PhoneLink_nativeStop<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    id: jint,
) {
    send(id, ClientMessage::Stop);
}

#[no_mangle]
pub extern "system" fn Java_com_pais_handy_mobile_PhoneLink_nativeCancel<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    id: jint,
) {
    send(id, ClientMessage::Cancel);
}

/// 16 kHz mono signed 16-bit little-endian PCM.
#[no_mangle]
pub extern "system" fn Java_com_pais_handy_mobile_PhoneLink_nativeSendAudio<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
    id: jint,
    pcm: JByteArray<'local>,
) {
    let Some(keyboard) = keyboard(id) else { return };
    let Ok(pcm) = env.convert_byte_array(&pcm) else {
        return;
    };
    let sender = keyboard
        .sender
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    if let Some(sender) = sender {
        let _ = sender.send_audio(pcm);
    }
}

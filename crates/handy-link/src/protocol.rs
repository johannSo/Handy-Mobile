//! The messages each side sends. Control messages are JSON text frames;
//! audio is binary frames of 16 kHz mono signed 16-bit little-endian PCM,
//! sent between [`ClientMessage::Start`] and [`ClientMessage::Stop`].

use serde::{Deserialize, Serialize};

/// Bumped when a change would confuse the other side. The desktop answers a
/// version it doesn't speak with [`ErrorCode::UnsupportedProtocol`].
pub const PROTOCOL_VERSION: u32 = 2;

/// Where the desktop sends a finished transcript. The phone gets the text
/// either way.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputMode {
    /// Paste it into the focused app on the desktop.
    #[default]
    Desktop,
    /// Only send it back to the phone.
    Phone,
}

/// What a peer can do beyond protocol 2 as first shipped. Each side lists
/// its own in the handshake; a peer that lists nothing is an older one.
pub mod feature {
    /// The desktop honours [`super::StartOptions`]: a raw transcription
    /// with the client's text settings, kept off the desktop's history and
    /// never pasted there.
    pub const RAW_TEXT: &str = "raw_text";
    /// The desktop answers [`super::ClientMessage::Prepare`].
    pub const PREPARE: &str = "prepare";
}

/// What kind of device pairs. Only informational: what a device may do is
/// decided by the desktop when it makes the pairing code.
pub mod device_kind {
    pub const PHONE: &str = "phone";
    pub const COMPUTER: &str = "computer";
}

/// How the client wants its speech turned into text. Fields left `None`
/// fall back to what the desktop is set to.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TextOptions {
    /// A language code, or `"auto"`.
    #[serde(default)]
    pub selected_language: Option<String>,
    #[serde(default)]
    pub spoken_languages: Option<Vec<String>>,
    #[serde(default)]
    pub translate_to_english: Option<bool>,
    /// Handy's language detection mode, as Handy spells it.
    #[serde(default)]
    pub language_detection: Option<String>,
    #[serde(default)]
    pub custom_words: Option<Vec<String>>,
    #[serde(default)]
    pub word_correction_threshold: Option<f64>,
    #[serde(default)]
    pub filler_word_removal: Option<bool>,
    #[serde(default)]
    pub custom_filler_words: Option<Vec<String>>,
    /// Handy's Chinese script setting, as Handy spells it.
    #[serde(default)]
    pub chinese_script: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StartOptions {
    /// Transcribe only: no history, no post-processing, no hook and no paste
    /// on the desktop. The client does those itself.
    #[serde(default)]
    pub raw: bool,
    #[serde(default)]
    pub text: Option<TextOptions>,
}

/// The model a desktop transcribes with, as far as a client needs to know.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    /// Language codes the model can be told to use.
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub supports_language_selection: bool,
    #[serde(default)]
    pub supports_translation: bool,
    /// Whether "auto" is a language it can be given.
    #[serde(default)]
    pub supports_language_detection: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// First message of a phone that isn't paired yet. `token` is the
    /// one-time token from the QR code.
    Pair {
        protocol: u32,
        token: String,
        device_name: String,
        /// See [`device_kind`]. A string, so a kind this desktop doesn't
        /// know can't fail the whole message.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        device_kind: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        features: Vec<String>,
    },
    /// First message of a paired phone.
    Auth {
        protocol: u32,
        device_id: String,
        secret: String,
        /// Which part of the phone app this is, such as its keyboard. A new
        /// connection replaces the phone's older one on the same channel
        /// and leaves its other channels alone.
        #[serde(default)]
        channel: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        features: Vec<String>,
    },
    /// Begin a recording. A `None` field means "what the desktop is set to".
    Start {
        #[serde(default)]
        output: Option<OutputMode>,
        #[serde(default)]
        post_process: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        options: Option<StartOptions>,
    },
    /// The recording is complete; transcribe it.
    Stop,
    /// Drop the recording, or the result of the transcription under way.
    Cancel,
    Ping,
    /// A recording is about to start: load the model now, so the text isn't
    /// kept waiting for it. Only sent to a desktop with [`feature::PREPARE`].
    Prepare,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Idle,
    Recording,
    Transcribing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The two apps are too far apart in version; one needs an update.
    UnsupportedProtocol,
    /// The pairing token is wrong, used or expired.
    InvalidToken,
    /// Not paired, unpaired from the desktop, or the first message wasn't
    /// `pair` or `auth`.
    Unauthorized,
    /// Too many failed attempts from this address; try again later.
    RateLimited,
    /// Another phone is dictating.
    Busy,
    /// The desktop has no model to transcribe with.
    NoModel,
    TranscriptionFailed,
    /// Transcribed, but the desktop couldn't paste it. The text still comes
    /// with the error.
    PasteFailed,
    /// The recording hit the length limit and was stopped.
    TooLong,
    BadRequest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Pairing worked. The phone keeps `device_id` and `secret` and is
    /// authenticated from here on.
    Paired {
        device_id: String,
        secret: String,
        desktop_name: String,
        desktop_version: String,
        default_output: OutputMode,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        features: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<ModelInfo>,
    },
    /// `auth` worked.
    Ready {
        desktop_name: String,
        desktop_version: String,
        default_output: OutputMode,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        features: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<ModelInfo>,
    },
    State {
        state: SessionState,
    },
    /// The transcript. Empty when nothing was said.
    Final {
        text: String,
        /// Whether the desktop pasted it.
        pasted: bool,
        /// The language it was transcribed in, when known.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        language: Option<String>,
        /// How that language was chosen, as Handy's history records it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        language_source: Option<String>,
    },
    /// The desktop switched models, or has none any more. Older clients
    /// ignore it.
    ModelChanged {
        #[serde(default)]
        model: Option<ModelInfo>,
    },
    Error {
        code: ErrorCode,
        #[serde(default)]
        message: Option<String>,
        /// The transcript, when the failure came after transcribing.
        #[serde(default)]
        text: Option<String>,
    },
    Pong,
}

impl ServerMessage {
    pub fn error(code: ErrorCode) -> Self {
        Self::Error {
            code,
            message: None,
            text: None,
        }
    }
}

/// PCM bytes off the wire, as the samples the transcriber takes. A trailing
/// odd byte is dropped.
pub fn pcm16_to_f32(bytes: &[u8], out: &mut Vec<f32>) {
    out.extend(
        bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| i16::from_le_bytes(*b) as f32 / 32768.0),
    );
}

pub fn f32_to_pcm16(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_keep_their_wire_names() {
        let json = serde_json::to_string(&ClientMessage::Start {
            output: Some(OutputMode::Phone),
            post_process: None,
            options: None,
        })
        .unwrap();
        // Exactly what protocol 2 always sent: older desktops read it.
        assert_eq!(
            json,
            r#"{"type":"start","output":"phone","post_process":null}"#
        );
        let parsed: ClientMessage = serde_json::from_str(r#"{"type":"start"}"#).unwrap();
        assert_eq!(
            parsed,
            ClientMessage::Start {
                output: None,
                post_process: None,
                options: None,
            }
        );
        let json = serde_json::to_string(&ServerMessage::State {
            state: SessionState::Transcribing,
        })
        .unwrap();
        assert_eq!(json, r#"{"type":"state","state":"transcribing"}"#);
    }

    #[test]
    fn pcm_survives_the_round_trip() {
        let samples = [0.0, 0.5, -0.5, 1.0, -1.0];
        let mut back = Vec::new();
        pcm16_to_f32(&f32_to_pcm16(&samples), &mut back);
        for (a, b) in samples.iter().zip(&back) {
            assert!((a - b).abs() < 1e-3, "{a} vs {b}");
        }
    }
}

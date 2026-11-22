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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// First message of a phone that isn't paired yet. `token` is the
    /// one-time token from the QR code.
    Pair {
        protocol: u32,
        token: String,
        device_name: String,
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
    },
    /// Begin a recording. A `None` field means "what the desktop is set to".
    Start {
        #[serde(default)]
        output: Option<OutputMode>,
        #[serde(default)]
        post_process: Option<bool>,
    },
    /// The recording is complete; transcribe it.
    Stop,
    /// Drop the recording, or the result of the transcription under way.
    Cancel,
    Ping,
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
    },
    /// `auth` worked.
    Ready {
        desktop_name: String,
        desktop_version: String,
        default_output: OutputMode,
    },
    State {
        state: SessionState,
    },
    /// The transcript. Empty when nothing was said.
    Final {
        text: String,
        /// Whether the desktop pasted it.
        pasted: bool,
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
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"type":"start","output":"phone","post_process":null}"#
        );
        let parsed: ClientMessage = serde_json::from_str(r#"{"type":"start"}"#).unwrap();
        assert_eq!(
            parsed,
            ClientMessage::Start {
                output: None,
                post_process: None
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

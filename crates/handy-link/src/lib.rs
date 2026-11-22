//! The link between Handy on a desktop and Handy Mobile.
//!
//! The phone is a remote microphone: it streams what it hears to the desktop,
//! which transcribes it with the model it already has. Both sides speak the
//! messages in [`protocol`] over a WebSocket inside TLS. The desktop's
//! certificate is self-signed; the phone learns its fingerprint from the
//! pairing QR code ([`pairing::PairingPayload`]) and accepts no other.
//!
//! - `server` feature: [`server::Server`], what the desktop runs.
//! - `client` feature: [`client`], what the phone runs.

pub mod discovery;
pub mod pairing;
pub mod protocol;
mod tls;

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;

#[cfg(feature = "server")]
pub use tls::Identity;

/// The port the desktop listens on unless the user picks another.
pub const DEFAULT_PORT: u16 = 47823;

/// Audio travels as 16 kHz mono signed 16-bit little-endian PCM.
pub const SAMPLE_RATE: u32 = 16_000;

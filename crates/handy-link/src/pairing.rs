//! What pairing exchanges: the payload in the QR code, and the secrets.

use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::net::IpAddr;

const SCHEME: &str = "handy://pair?";

/// Everything a phone needs to find the desktop and trust it, as shown in the
/// QR code: `handy://pair?v=1&id=..&n=..&a=ip,ip&p=port&f=..&t=..`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairingPayload {
    /// Stays the same across pairings; the phone asks for it by this when it
    /// looks for the desktop on the network.
    pub desktop_id: String,
    pub desktop_name: String,
    /// Every address the desktop has, likeliest first.
    pub addresses: Vec<IpAddr>,
    pub port: u16,
    /// SHA-256 of the desktop's certificate, in hex.
    pub fingerprint: String,
    /// Good for one pairing, for a couple of minutes.
    pub token: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PairingPayloadError {
    NotAPairingCode,
    /// Made by a newer Handy than this one understands.
    UnsupportedVersion,
    Incomplete,
}

impl std::fmt::Display for PairingPayloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NotAPairingCode => "not a Handy pairing code",
            Self::UnsupportedVersion => "pairing code from a newer version of Handy",
            Self::Incomplete => "pairing code is incomplete",
        })
    }
}

impl std::error::Error for PairingPayloadError {}

impl PairingPayload {
    pub fn encode(&self) -> String {
        let addresses = self
            .addresses
            .iter()
            .map(|a| a.to_string())
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{SCHEME}v=1&id={}&n={}&a={}&p={}&f={}&t={}",
            percent_encode(&self.desktop_id),
            percent_encode(&self.desktop_name),
            percent_encode(&addresses),
            self.port,
            percent_encode(&self.fingerprint),
            percent_encode(&self.token),
        )
    }

    pub fn decode(code: &str) -> Result<Self, PairingPayloadError> {
        let query = code
            .trim()
            .strip_prefix(SCHEME)
            .ok_or(PairingPayloadError::NotAPairingCode)?;
        let mut version = None;
        let mut desktop_id = None;
        let mut desktop_name = None;
        let mut addresses = Vec::new();
        let mut port = None;
        let mut fingerprint = None;
        let mut token = None;
        for pair in query.split('&') {
            let Some((key, value)) = pair.split_once('=') else {
                continue;
            };
            let value = percent_decode(value).ok_or(PairingPayloadError::Incomplete)?;
            match key {
                "v" => version = Some(value),
                "id" => desktop_id = Some(value),
                "n" => desktop_name = Some(value),
                "a" => addresses = value.split(',').filter_map(|a| a.parse().ok()).collect(),
                "p" => port = value.parse::<u16>().ok(),
                "f" => fingerprint = Some(value.to_ascii_lowercase()),
                "t" => token = Some(value),
                // A newer desktop may say more; what's here is still enough.
                _ => {}
            }
        }
        match version.as_deref() {
            Some("1") => {}
            Some(_) => return Err(PairingPayloadError::UnsupportedVersion),
            None => return Err(PairingPayloadError::Incomplete),
        }
        let fingerprint = fingerprint
            .filter(|f| f.len() == 64 && f.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or(PairingPayloadError::Incomplete)?;
        if addresses.is_empty() {
            return Err(PairingPayloadError::Incomplete);
        }
        Ok(Self {
            desktop_id: desktop_id
                .filter(|s| !s.is_empty())
                .ok_or(PairingPayloadError::Incomplete)?,
            desktop_name: desktop_name.unwrap_or_default(),
            addresses,
            port: port
                .filter(|p| *p != 0)
                .ok_or(PairingPayloadError::Incomplete)?,
            fingerprint,
            token: token
                .filter(|s| !s.is_empty())
                .ok_or(PairingPayloadError::Incomplete)?,
        })
    }
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b',' | b':' => {
                out.push(b as char)
            }
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

pub fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

pub fn from_hex(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
        .collect()
}

/// `bytes` random bytes from the OS, in hex.
pub fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).expect("the OS has no random number source");
    to_hex(&buf)
}

/// A paired phone's secret: 256 random bits.
pub fn new_secret() -> String {
    random_hex(32)
}

/// What the desktop keeps of a secret. The secret is random and long, so a
/// plain hash is enough: there is no password here to guess.
pub fn hash_secret(secret: &str) -> String {
    to_hex(&Sha256::digest(secret.as_bytes()))
}

/// Whether `secret` is the one `hash` was made from, in time that doesn't
/// depend on how much of it matches.
pub fn secret_matches(secret: &str, hash: &str) -> bool {
    constant_time_eq(hash_secret(secret).as_bytes(), hash.as_bytes())
}

pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// SHA-256 of a certificate, in hex: what the QR code carries and the phone
/// pins.
pub fn fingerprint(cert_der: &[u8]) -> String {
    to_hex(&Sha256::digest(cert_der))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload() -> PairingPayload {
        PairingPayload {
            desktop_id: "0123abcd".into(),
            desktop_name: "Jo's Büro & PC".into(),
            addresses: vec!["192.168.1.20".parse().unwrap(), "fe80::1".parse().unwrap()],
            port: 47823,
            fingerprint: "ab".repeat(32),
            token: "deadbeef".into(),
        }
    }

    #[test]
    fn payload_survives_the_qr_code() {
        let payload = payload();
        let code = payload.encode();
        assert!(code.starts_with("handy://pair?v=1&"));
        assert!(!code.contains(' '));
        assert_eq!(PairingPayload::decode(&format!("  {code}\n")), Ok(payload));
    }

    #[test]
    fn other_codes_are_refused() {
        assert_eq!(
            PairingPayload::decode("https://example.com"),
            Err(PairingPayloadError::NotAPairingCode)
        );
        assert_eq!(
            PairingPayload::decode(&payload().encode().replace("v=1", "v=2")),
            Err(PairingPayloadError::UnsupportedVersion)
        );
        assert_eq!(
            PairingPayload::decode("handy://pair?v=1&id=a&p=1&t=x"),
            Err(PairingPayloadError::Incomplete)
        );
        // A fingerprint that isn't a SHA-256 can't pin anything.
        let short = payload().encode().replace(&"ab".repeat(32), "abab");
        assert_eq!(
            PairingPayload::decode(&short),
            Err(PairingPayloadError::Incomplete)
        );
    }

    #[test]
    fn only_the_right_secret_matches() {
        let secret = new_secret();
        assert_eq!(secret.len(), 64);
        let hash = hash_secret(&secret);
        assert!(secret_matches(&secret, &hash));
        assert!(!secret_matches(&new_secret(), &hash));
        assert!(!secret_matches(&secret, ""));
    }

    #[test]
    fn hex_round_trips() {
        assert_eq!(
            from_hex(&to_hex(&[0, 1, 254, 255])),
            Some(vec![0, 1, 254, 255])
        );
        assert_eq!(from_hex("abc"), None);
        assert_eq!(from_hex("zz"), None);
    }
}

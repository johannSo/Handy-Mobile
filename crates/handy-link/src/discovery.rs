//! Finding the desktop again after its address changed.
//!
//! The phone broadcasts "where is desktop X?" over UDP and the desktop with
//! that id answers with its port; the phone then connects to wherever the
//! answer came from. Nothing here is trusted: an answer only names a place to
//! try, and the TLS pin decides whether it's really the desktop.
//!
//! A broadcast rather than mDNS because a phone needs no extra permission or
//! multicast lock to send one and hear the unicast reply.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use tokio::net::UdpSocket;

const ASK: &str = "HANDY-WHERE 1";
const ANSWER: &str = "HANDY-HERE 1";

fn ask(desktop_id: &str) -> String {
    format!("{ASK} {desktop_id}")
}

/// The desktop's answer to `datagram`, if it asks for `desktop_id`.
pub fn answer_to(datagram: &[u8], desktop_id: &str, port: u16) -> Option<String> {
    let text = std::str::from_utf8(datagram).ok()?;
    (text.trim() == ask(desktop_id)).then(|| format!("{ANSWER} {desktop_id} {port}"))
}

fn parse_answer(datagram: &[u8], desktop_id: &str) -> Option<u16> {
    let text = std::str::from_utf8(datagram).ok()?;
    let rest = text.trim().strip_prefix(ANSWER)?.trim_start();
    let (id, port) = rest.split_once(' ')?;
    (id == desktop_id).then(|| port.parse().ok()).flatten()
}

/// Answers phones looking for `desktop_id` until dropped (it runs as a task;
/// abort it to stop). Bound to the same port number as the server, over UDP.
pub async fn respond(desktop_id: String, port: u16) -> std::io::Result<()> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port)).await?;
    let mut buf = [0u8; 256];
    loop {
        let Ok((len, from)) = socket.recv_from(&mut buf).await else {
            continue;
        };
        if let Some(answer) = answer_to(&buf[..len], &desktop_id, port) {
            let _ = socket.send_to(answer.as_bytes(), from).await;
        }
    }
}

/// This device's address on the network it reaches others through, if any.
/// No packet is sent: connecting a UDP socket only picks a route.
pub fn local_ipv4() -> Option<Ipv4Addr> {
    let socket = std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ip) if !ip.is_unspecified() && !ip.is_loopback() => Some(ip),
        _ => None,
    }
}

/// Where to shout: everyone on the link, plus the usual /24 around this
/// device and around each place the desktop used to be.
fn broadcast_targets(known: &[IpAddr]) -> Vec<Ipv4Addr> {
    let mut targets = vec![Ipv4Addr::BROADCAST];
    let around = |ip: Ipv4Addr| {
        let [a, b, c, _] = ip.octets();
        Ipv4Addr::new(a, b, c, 255)
    };
    let nearby = local_ipv4()
        .into_iter()
        .chain(known.iter().filter_map(|ip| match ip {
            IpAddr::V4(ip) => Some(*ip),
            IpAddr::V6(_) => None,
        }));
    for ip in nearby {
        let target = around(ip);
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
    targets
}

/// Asks the network where `desktop_id` is, on the server's `port`. Returns the
/// first address that answers within `wait`.
pub async fn find(
    desktop_id: &str,
    port: u16,
    known: &[IpAddr],
    wait: Duration,
) -> Option<SocketAddr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await.ok()?;
    socket.set_broadcast(true).ok()?;
    let question = ask(desktop_id);
    let mut sent = false;
    for target in broadcast_targets(known) {
        sent |= socket
            .send_to(question.as_bytes(), (target, port))
            .await
            .is_ok();
    }
    if !sent {
        return None;
    }
    let mut buf = [0u8; 256];
    tokio::time::timeout(wait, async {
        loop {
            let (len, from) = socket.recv_from(&mut buf).await.ok()?;
            if let Some(port) = parse_answer(&buf[..len], desktop_id) {
                return Some(SocketAddr::new(from.ip(), port));
            }
        }
    })
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_asked_desktop_answers() {
        let question = ask("abc");
        let answer = answer_to(question.as_bytes(), "abc", 4242).unwrap();
        assert_eq!(parse_answer(answer.as_bytes(), "abc"), Some(4242));
        assert_eq!(answer_to(question.as_bytes(), "other", 4242), None);
        assert_eq!(parse_answer(answer.as_bytes(), "other"), None);
        assert_eq!(answer_to(b"\xff\xfe", "abc", 1), None);
    }

    #[test]
    fn broadcasts_cover_the_last_known_network() {
        let targets = broadcast_targets(&["10.1.2.3".parse().unwrap()]);
        assert_eq!(targets[0], Ipv4Addr::BROADCAST);
        assert!(targets.contains(&Ipv4Addr::new(10, 1, 2, 255)));
    }
}

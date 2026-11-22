//! A phone in a terminal, for testing a desktop without one.
//!
//!   link-cli pair '<handy://pair?...>' [state-file]
//!   link-cli send <16 kHz mono 16-bit .wav> [phone|desktop] [state-file]
//!
//! `pair` writes what a phone would remember to the state file (default
//! `link-cli.json`); `send` streams the file in real-time-sized frames and
//! prints what the desktop answers.

use handy_link::client::{self, Desktop};
use handy_link::pairing::PairingPayload;
use handy_link::protocol::{ClientMessage, OutputMode, ServerMessage, SessionState};
use std::time::Duration;

const DEFAULT_STATE: &str = "link-cli.json";

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("pair") => {
            let code = args
                .get(1)
                .ok_or("usage: link-cli pair <code> [state-file]")?;
            let state = args.get(2).map_or(DEFAULT_STATE, String::as_str);
            let payload = PairingPayload::decode(code)?;
            let (desktop, info, _link) = client::pair(&payload, "link-cli").await?;
            std::fs::write(state, serde_json::to_string_pretty(&desktop)?)?;
            println!("Paired with {} (Handy {})", info.name, info.version);
        }
        Some("send") => {
            let wav = args
                .get(1)
                .ok_or("usage: link-cli send <wav> [phone|desktop] [state-file]")?;
            let output = match args.get(2).map(String::as_str) {
                Some("phone") => Some(OutputMode::Phone),
                Some("desktop") => Some(OutputMode::Desktop),
                _ => None,
            };
            let state = args.get(3).map_or(DEFAULT_STATE, String::as_str);
            let mut desktop: Desktop = serde_json::from_str(&std::fs::read_to_string(state)?)?;
            let pcm = read_pcm(wav)?;

            let (info, mut link) = client::connect(&mut desktop, "cli").await?;
            println!("Connected to {} (Handy {})", info.name, info.version);
            // Remember where it was found, as a phone would.
            std::fs::write(state, serde_json::to_string_pretty(&desktop)?)?;

            link.send(ClientMessage::Start {
                output,
                post_process: None,
            })?;
            // 100 ms frames, sent faster than real time: a test needn't wait.
            for frame in pcm.chunks(3200) {
                link.send_audio(frame.to_vec())?;
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            link.send(ClientMessage::Stop)?;
            while let Some(message) = link.recv().await {
                match message {
                    ServerMessage::Final { text, pasted } => {
                        println!("pasted on desktop: {pasted}\n{text}");
                    }
                    ServerMessage::Error {
                        code,
                        message,
                        text,
                    } => {
                        println!("error: {code:?} {}", message.unwrap_or_default());
                        if let Some(text) = text {
                            println!("{text}");
                        }
                        if matches!(code, handy_link::protocol::ErrorCode::TooLong) {
                            continue;
                        }
                    }
                    ServerMessage::State {
                        state: SessionState::Idle,
                    } => break,
                    ServerMessage::State { state } => println!("state: {state:?}"),
                    _ => {}
                }
            }
        }
        _ => {
            eprintln!("usage: link-cli pair <code> [state-file]\n       link-cli send <wav> [phone|desktop] [state-file]");
            std::process::exit(2);
        }
    }
    Ok(())
}

fn read_pcm(path: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    if spec.sample_rate != handy_link::SAMPLE_RATE
        || spec.channels != 1
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
    {
        return Err(format!(
            "{path} must be 16 kHz mono 16-bit PCM (it is {} Hz, {} channel(s), {} bit)",
            spec.sample_rate, spec.channels, spec.bits_per_sample
        )
        .into());
    }
    let mut pcm = Vec::new();
    for sample in reader.samples::<i16>() {
        pcm.extend_from_slice(&sample?.to_le_bytes());
    }
    Ok(pcm)
}

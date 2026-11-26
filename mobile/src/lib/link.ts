import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// These mirror the Rust side (mobile/src-tauri/src/lib.rs and the protocol
// in crates/handy-link).

export type LinkState =
  | "unpaired"
  | "connecting"
  | "connected"
  | "offline"
  | "rejected"
  | "incompatible";

export type SessionState = "idle" | "recording" | "transcribing";

export type OutputMode = "desktop" | "phone";

export type ErrorCode =
  | "unsupported_protocol"
  | "invalid_token"
  | "unauthorized"
  | "rate_limited"
  | "busy"
  | "no_model"
  | "transcription_failed"
  | "paste_failed"
  | "too_long"
  | "bad_request";

export interface Snapshot {
  link: LinkState;
  desktop_name: string | null;
  desktop_version: string | null;
  default_output: OutputMode | null;
  session: SessionState;
  /** Whether the computer lets this phone type on it; `null` if it didn't say. */
  can_type: boolean | null;
  detail: string | null;
}

export interface TranscriptEvent {
  text: string;
  pasted: boolean;
}

export interface LinkErrorEvent {
  code: ErrorCode;
  message: string | null;
  text: string | null;
}

/** Why pairing failed, as the core reports it. */
export type PairError =
  | "invalid_code"
  | "expired_code"
  | "incompatible"
  | "rate_limited"
  | "unreachable"
  | "untrusted"
  | "failed";

export const getSnapshot = () => invoke<Snapshot>("get_snapshot");

export const pair = (code: string, deviceName: string) =>
  invoke<void>("pair", { code, deviceName });

export const unpair = () => invoke<void>("unpair");

export const reconnect = () => invoke<void>("reconnect");

export const startRecording = (output: OutputMode | null) =>
  invoke<void>("start_recording", { output, postProcess: null });

export const stopRecording = () => invoke<void>("stop_recording");

export const cancelRecording = () => invoke<void>("cancel_recording");

/** One frame of 16 kHz mono 16-bit PCM, sent as the call's raw body. */
export const sendAudio = (pcm: ArrayBuffer) =>
  invoke<void>("send_audio", new Uint8Array(pcm));

export const onSnapshot = (handler: (snapshot: Snapshot) => void) =>
  listen<Snapshot>("link-snapshot", (event) => handler(event.payload));

export const onTranscript = (handler: (event: TranscriptEvent) => void) =>
  listen<TranscriptEvent>("transcript", (event) => handler(event.payload));

export const onLinkError = (handler: (event: LinkErrorEvent) => void) =>
  listen<LinkErrorEvent>("link-error", (event) => handler(event.payload));

export type { UnlistenFn };

/** A name for this phone that its owner will recognise in the desktop's list. */
export const deviceName = (): string => {
  // "Mozilla/5.0 (Linux; Android 15; Pixel 9 Build/...; wv) ..."
  const match = navigator.userAgent.match(
    /Android [^;)]+;\s*([^;)]+?)(?:\s+Build\/[^;)]*)?[;)]/,
  );
  const model = match?.[1]?.trim();
  if (model && model !== "K" && model.toLowerCase() !== "wv") return model;
  if (/iPhone/.test(navigator.userAgent)) return "iPhone";
  if (/iPad/.test(navigator.userAgent)) return "iPad";
  return /Android/.test(navigator.userAgent) ? "Android phone" : "Phone";
};

import { useCallback, useEffect, useRef, useState } from "react";
import {
  cancelRecording,
  onLinkError,
  onTranscript,
  sendAudio,
  startRecording,
  stopRecording,
  type ErrorCode,
  type OutputMode,
  type Snapshot,
} from "./link";
import { MicFailure, startRecorder, type Recorder } from "./recorder";

/** What the microphone on this phone is doing. */
export type MicPhase = "idle" | "starting" | "recording" | "finishing";

export type NoticeKey =
  | "noSpeech"
  | "micDenied"
  | "micUnavailable"
  | "connectionLost"
  | "generic"
  | Extract<
      ErrorCode,
      "busy" | "no_model" | "transcription_failed" | "paste_failed" | "too_long"
    >;

export interface Transcript {
  id: number;
  text: string;
  pasted: boolean;
  at: number;
}

const HISTORY_KEY = "handy-mobile.transcripts";
const MAX_HISTORY = 30;
/** Kept recording after the tap, so the last syllable isn't cut off. */
const STOP_TAIL_MS = 200;

const loadHistory = (): Transcript[] => {
  try {
    const stored = JSON.parse(localStorage.getItem(HISTORY_KEY) ?? "[]");
    return Array.isArray(stored) ? stored.slice(0, MAX_HISTORY) : [];
  } catch {
    return [];
  }
};

const NOTICE_CODES: ErrorCode[] = [
  "busy",
  "no_model",
  "transcription_failed",
  "paste_failed",
  "too_long",
];

/**
 * A dictation, start to finish: opens the microphone, streams it to the
 * desktop, and collects what comes back.
 */
export const useDictation = (snapshot: Snapshot, output: OutputMode | null) => {
  const [mic, setMic] = useState<MicPhase>("idle");
  const [level, setLevel] = useState(0);
  const [startedAt, setStartedAt] = useState<number | null>(null);
  const [notice, setNotice] = useState<NoticeKey | null>(null);
  const [transcripts, setTranscripts] = useState<Transcript[]>(loadHistory);

  const recorder = useRef<Recorder | null>(null);
  // Frames go out one at a time, each after the last: the calls are
  // asynchronous, and two in flight could arrive in the wrong order.
  const sending = useRef<Promise<void>>(Promise.resolve());
  // Bumped whenever a recording ends, so a start that is still waiting for
  // the microphone knows it has been overtaken.
  const attempt = useRef(0);
  const micRef = useRef<MicPhase>("idle");
  micRef.current = mic;

  const connected = snapshot.link === "connected";

  useEffect(() => {
    localStorage.setItem(HISTORY_KEY, JSON.stringify(transcripts));
  }, [transcripts]);

  const closeMic = useCallback(async () => {
    attempt.current += 1;
    const active = recorder.current;
    recorder.current = null;
    setLevel(0);
    setStartedAt(null);
    await active?.stop();
  }, []);

  /** Ends the recording without asking for a transcript. */
  const abort = useCallback(
    async (why: NoticeKey | null) => {
      await closeMic();
      setMic("idle");
      if (why) setNotice(why);
    },
    [closeMic],
  );

  const start = useCallback(async () => {
    if (micRef.current !== "idle" || !connected) return;
    setNotice(null);
    setMic("starting");
    const mine = ++attempt.current;
    try {
      // The desktop first, so it can refuse before the microphone opens.
      await startRecording(output);
      sending.current = Promise.resolve();
      const started = await startRecorder({
        onFrame: (pcm) => {
          sending.current = sending.current
            .then(() => sendAudio(pcm))
            .catch(() => undefined);
        },
        onLevel: setLevel,
      });
      if (attempt.current !== mine) {
        // Cancelled, or refused by the desktop, while the microphone opened.
        await started.stop();
        return;
      }
      recorder.current = started;
      setStartedAt(Date.now());
      setMic("recording");
    } catch (error) {
      if (attempt.current !== mine) return;
      void cancelRecording().catch(() => undefined);
      await abort(
        error instanceof MicFailure
          ? error.reason === "denied"
            ? "micDenied"
            : "micUnavailable"
          : "connectionLost",
      );
    }
  }, [abort, connected, output]);

  const finish = useCallback(async () => {
    if (micRef.current !== "recording") return;
    setMic("finishing");
    await new Promise((resolve) => setTimeout(resolve, STOP_TAIL_MS));
    await closeMic();
    // Every frame has to be on its way before the desktop is told to stop.
    await sending.current;
    try {
      await stopRecording();
    } catch {
      setNotice("connectionLost");
    }
    setMic("idle");
  }, [closeMic]);

  const cancel = useCallback(async () => {
    void cancelRecording().catch(() => undefined);
    await abort(null);
  }, [abort]);

  // What the desktop says back.
  useEffect(() => {
    const transcript = onTranscript(({ text, pasted }) => {
      if (text.trim() === "") {
        setNotice("noSpeech");
        return;
      }
      setTranscripts((list) =>
        [{ id: Date.now(), text, pasted, at: Date.now() }, ...list].slice(
          0,
          MAX_HISTORY,
        ),
      );
    });
    const failure = onLinkError(({ code, text }) => {
      setNotice(NOTICE_CODES.includes(code) ? (code as NoticeKey) : "generic");
      // Only the paste failed: the words are still worth having.
      if (text) {
        setTranscripts((list) =>
          [
            { id: Date.now(), text, pasted: false, at: Date.now() },
            ...list,
          ].slice(0, MAX_HISTORY),
        );
      }
      // The desktop refused or cut the recording short; the microphone has
      // nothing left to do. (`too_long` is transcribed by the desktop.)
      if (micRef.current !== "idle") void closeMic().then(() => setMic("idle"));
    });
    return () => {
      void transcript.then((stop) => stop());
      void failure.then((stop) => stop());
    };
  }, [closeMic]);

  // A dropped link takes the recording with it.
  useEffect(() => {
    if (!connected && micRef.current !== "idle") {
      void abort("connectionLost");
    }
  }, [abort, connected]);

  // Android stops a WebView's microphone in the background, so leaving the
  // app finishes the dictation instead of recording silence. Only a running
  // recording, though: while one is starting, the page is also hidden by the
  // system's own "allow the microphone?" dialog.
  useEffect(() => {
    const onHidden = () => {
      if (
        document.visibilityState === "hidden" &&
        micRef.current === "recording"
      ) {
        void finish();
      }
    };
    document.addEventListener("visibilitychange", onHidden);
    return () => document.removeEventListener("visibilitychange", onHidden);
  }, [finish]);

  // The microphone must not outlive the screen.
  useEffect(() => () => void recorder.current?.stop(), []);

  return {
    mic,
    level,
    startedAt,
    notice,
    dismissNotice: () => setNotice(null),
    transcripts,
    clearTranscripts: () => setTranscripts([]),
    start,
    finish,
    cancel,
  };
};

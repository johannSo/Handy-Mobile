// The microphone, as a stream of 16 kHz mono PCM frames.

export type MicError = "denied" | "unavailable";

export class MicFailure extends Error {
  constructor(public readonly reason: MicError) {
    super(reason);
  }
}

export interface Recorder {
  /** Stops the microphone. Frames already captured have been delivered. */
  stop: () => Promise<void>;
}

interface Handlers {
  /** 100 ms of audio, in order. */
  onFrame: (pcm: ArrayBuffer) => void;
  /** Loudness of the last frame, 0 to 1, for the meter. */
  onLevel: (level: number) => void;
}

export const startRecorder = async (handlers: Handlers): Promise<Recorder> => {
  if (!navigator.mediaDevices?.getUserMedia) {
    throw new MicFailure("unavailable");
  }

  let stream: MediaStream;
  try {
    stream = await navigator.mediaDevices.getUserMedia({
      audio: {
        channelCount: 1,
        // The phone is held to the mouth, with nothing playing back: echo
        // cancellation would only shave the speech. Noise suppression and
        // gain control do help a phone microphone in a room.
        echoCancellation: false,
        noiseSuppression: true,
        autoGainControl: true,
      },
    });
  } catch (error) {
    const name = error instanceof DOMException ? error.name : "";
    throw new MicFailure(
      name === "NotAllowedError" || name === "SecurityError"
        ? "denied"
        : "unavailable",
    );
  }

  let context: AudioContext | null = null;
  try {
    // Ask for the rate the desktop wants; where the device won't give it,
    // the worklet resamples from whatever it does give.
    try {
      context = new AudioContext({ sampleRate: 16000 });
    } catch {
      context = new AudioContext();
    }
    await context.audioWorklet.addModule("/pcm-worklet.js");
    const source = context.createMediaStreamSource(stream);
    const capture = new AudioWorkletNode(context, "pcm-capture", {
      numberOfInputs: 1,
      numberOfOutputs: 0,
      channelCount: 1,
      channelCountMode: "explicit",
    });
    capture.port.onmessage = (
      event: MessageEvent<{ pcm: ArrayBuffer; peak: number }>,
    ) => {
      handlers.onFrame(event.data.pcm);
      handlers.onLevel(event.data.peak);
    };
    source.connect(capture);
    if (context.state === "suspended") {
      await context.resume();
    }

    const running = context;
    return {
      stop: async () => {
        capture.port.onmessage = null;
        source.disconnect();
        stream.getTracks().forEach((track) => track.stop());
        await running.close().catch(() => undefined);
      },
    };
  } catch {
    stream.getTracks().forEach((track) => track.stop());
    await context?.close().catch(() => undefined);
    throw new MicFailure("unavailable");
  }
};

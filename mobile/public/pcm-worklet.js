// Runs on the audio thread. Turns whatever the microphone delivers into
// 16 kHz mono 16-bit PCM, 100 ms at a time, which is what the desktop takes.
const TARGET_RATE = 16000;
const FRAME_SAMPLES = 1600;

class PcmCapture extends AudioWorkletProcessor {
  constructor() {
    super();
    // `sampleRate` is the context's rate, a global in this scope.
    this.step = sampleRate / TARGET_RATE;
    // Where the next output sample falls, in input samples, relative to the
    // start of the next block. Carried across blocks so the resampling has
    // no seam every 128 samples.
    this.position = 0;
    this.previous = 0;
    this.frame = new Int16Array(FRAME_SAMPLES);
    this.filled = 0;
    this.peak = 0;
  }

  push(sample) {
    const clamped = Math.max(-1, Math.min(1, sample));
    this.peak = Math.max(this.peak, Math.abs(clamped));
    this.frame[this.filled++] = Math.round(clamped * 32767);
    if (this.filled === FRAME_SAMPLES) {
      const pcm = this.frame.buffer;
      this.port.postMessage({ pcm, peak: this.peak }, [pcm]);
      this.frame = new Int16Array(FRAME_SAMPLES);
      this.filled = 0;
      this.peak = 0;
    }
  }

  process(inputs) {
    const input = inputs[0] && inputs[0][0];
    if (!input || input.length === 0) return true;

    if (this.step === 1) {
      for (let i = 0; i < input.length; i++) this.push(input[i]);
      return true;
    }

    // Linear interpolation between neighbouring input samples. Speech sits
    // far below the 8 kHz this keeps, so nothing sharper is needed.
    while (this.position < input.length) {
      const index = Math.floor(this.position);
      const fraction = this.position - index;
      const a = index === 0 ? this.previous : input[index - 1];
      const b = input[index];
      this.push(a + (b - a) * fraction);
      this.position += this.step;
    }
    this.position -= input.length;
    this.previous = input[input.length - 1];
    return true;
  }
}

registerProcessor("pcm-capture", PcmCapture);

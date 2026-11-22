package com.pais.handy.mobile

import android.annotation.SuppressLint
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import kotlin.math.sqrt

/**
 * The microphone, as the computer wants it: 16 kHz mono 16-bit PCM, sent a
 * tenth of a second at a time.
 */
class Recorder(private val link: PhoneLink, private val onLevel: (Float) -> Unit) {
  @Volatile private var running = false
  private var sendStop = false
  private var finished = false
  private var record: AudioRecord? = null

  @SuppressLint("MissingPermission")
  fun start(): Boolean {
    val smallest = AudioRecord.getMinBufferSize(
      RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT,
    )
    if (smallest <= 0) return false
    val record = try {
      AudioRecord(
        MediaRecorder.AudioSource.VOICE_RECOGNITION, RATE,
        AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT,
        maxOf(smallest, FRAME * 2 * 4),
      )
    } catch (_: Exception) {
      return false
    }
    if (record.state != AudioRecord.STATE_INITIALIZED) {
      record.release()
      return false
    }
    try {
      record.startRecording()
    } catch (_: Exception) {
      record.release()
      return false
    }
    if (record.recordingState != AudioRecord.RECORDSTATE_RECORDING) {
      record.release()
      return false
    }
    this.record = record
    running = true
    link.start()
    Thread({ pump(record) }, "handy-mic").start()
    return true
  }

  /** Stops recording. With [send], the computer is told to transcribe. */
  fun stop(send: Boolean) {
    synchronized(this) {
      // If the microphone gave out early, nobody is left to say so.
      if (finished) {
        if (send) link.stop()
        return
      }
      sendStop = send
      running = false
    }
  }

  private fun pump(record: AudioRecord) {
    val samples = ShortArray(FRAME)
    while (running) {
      val read = record.read(samples, 0, FRAME)
      if (read <= 0) break
      val bytes = ByteArray(read * 2)
      var energy = 0.0
      for (i in 0 until read) {
        val sample = samples[i].toInt()
        bytes[i * 2] = sample.toByte()
        bytes[i * 2 + 1] = (sample shr 8).toByte()
        energy += sample.toDouble() * sample
      }
      link.sendAudio(bytes)
      onLevel((sqrt(energy / read) / 32768.0).toFloat())
    }
    try {
      record.stop()
    } catch (_: Exception) {
    }
    record.release()
    synchronized(this) {
      finished = true
      // After the last audio, so the computer has all of it.
      if (sendStop) link.stop()
    }
  }

  private companion object {
    const val RATE = 16000
    const val FRAME = RATE / 10
  }
}

package com.pais.handy.mobile

import android.os.Handler
import android.os.Looper
import org.json.JSONObject

/**
 * The keyboard's connection to the computer. The work is done in Rust
 * (`src-tauri/src/keyboard.rs`); this passes calls down and events back up.
 */
object KeyboardLink {
  init {
    // The same library the app runs on. Loading it doesn't start the app.
    System.loadLibrary("handy_mobile_lib")
  }

  /** Called on the main thread with each event. Events are JSON with a `type`. */
  var listener: ((JSONObject) -> Unit)? = null

  private val main = Handler(Looper.getMainLooper())
  private var listening = false

  /** Connects to the computer paired in [desktopFile], and reconnects as needed. */
  fun open(desktopFile: String) {
    if (!listening) {
      listening = true
      Thread({
        while (true) {
          val json = nativeNextEvent() ?: break
          val event = try { JSONObject(json) } catch (_: Exception) { continue }
          main.post { listener?.invoke(event) }
        }
      }, "handy-keyboard-events").apply { isDaemon = true }.start()
    }
    nativeOpen(desktopFile)
  }

  fun close() = nativeClose()
  fun retry() = nativeRetry()
  fun start() = nativeStart()
  fun stop() = nativeStop()
  fun cancel() = nativeCancel()
  fun sendAudio(pcm: ByteArray) = nativeSendAudio(pcm)

  @JvmStatic private external fun nativeOpen(path: String)
  @JvmStatic private external fun nativeClose()
  @JvmStatic private external fun nativeRetry()
  @JvmStatic private external fun nativeNextEvent(): String?
  @JvmStatic private external fun nativeStart()
  @JvmStatic private external fun nativeStop()
  @JvmStatic private external fun nativeCancel()
  @JvmStatic private external fun nativeSendAudio(pcm: ByteArray)
}

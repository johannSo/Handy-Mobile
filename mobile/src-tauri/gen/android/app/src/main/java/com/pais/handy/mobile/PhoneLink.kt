package com.pais.handy.mobile

import android.os.Handler
import android.os.Looper
import org.json.JSONObject

/**
 * A connection to the computer for something that runs outside the app's
 * window: the keyboard or the floating bubble. Each has its own. The work is
 * done in Rust (`src-tauri/src/keyboard.rs`); this passes calls down and
 * events back up.
 */
class PhoneLink private constructor(private val id: Int) {
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
          val json = nativeNextEvent(id) ?: break
          val event = try { JSONObject(json) } catch (_: Exception) { continue }
          main.post { listener?.invoke(event) }
        }
      }, "handy-link-events-$id").apply { isDaemon = true }.start()
    }
    nativeOpen(id, desktopFile)
  }

  fun close() = nativeClose(id)
  fun retry() = nativeRetry(id)
  fun start() = nativeStart(id)
  fun stop() = nativeStop(id)
  fun cancel() = nativeCancel(id)
  fun sendAudio(pcm: ByteArray) = nativeSendAudio(id, pcm)

  companion object {
    init {
      // The same library the app runs on. Loading it doesn't start the app.
      System.loadLibrary("handy_mobile_lib")
    }

    // Numbered as `CHANNELS` is in keyboard.rs.
    val keyboard = PhoneLink(0)
    val bubble = PhoneLink(1)

    @JvmStatic private external fun nativeOpen(id: Int, path: String)
    @JvmStatic private external fun nativeClose(id: Int)
    @JvmStatic private external fun nativeRetry(id: Int)
    @JvmStatic private external fun nativeNextEvent(id: Int): String?
    @JvmStatic private external fun nativeStart(id: Int)
    @JvmStatic private external fun nativeStop(id: Int)
    @JvmStatic private external fun nativeCancel(id: Int)
    @JvmStatic private external fun nativeSendAudio(id: Int, pcm: ByteArray)
  }
}

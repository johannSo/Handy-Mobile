package com.pais.handy.mobile

import android.Manifest
import android.annotation.SuppressLint
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.res.ColorStateList
import android.content.res.Configuration
import android.graphics.Color
import android.graphics.drawable.GradientDrawable
import android.inputmethodservice.InputMethodService
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.view.Gravity
import android.view.HapticFeedbackConstants
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.View
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodManager
import android.widget.FrameLayout
import android.widget.ImageButton
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.TextView
import org.json.JSONObject
import kotlin.math.min

/**
 * The Handy keyboard: a microphone where the keys would be. What it hears is
 * transcribed on the paired computer and typed into the focused text field.
 *
 * It is connected only while it's on screen. The link itself is in
 * [PhoneLink]; the pairing it uses is the app's.
 */
class HandyKeyboardService : InputMethodService() {
  private enum class Phase { IDLE, RECORDING, TRANSCRIBING }

  private val main = Handler(Looper.getMainLooper())
  private val link = PhoneLink.keyboard
  private var linkState = "connecting"
  private var computer: String? = null
  private var phase = Phase.IDLE
  /** A passing message, shown in place of the status until it times out. */
  private var notice: String? = null
  private val clearNotice = Runnable { notice = null; render() }
  private var recorder: Recorder? = null

  private var status: TextView? = null
  private var mic: ImageButton? = null
  private var halo: View? = null
  private var spinner: ProgressBar? = null
  private var cancel: ImageButton? = null
  private var palette = Palette(false)

  // Views

  private class Palette(night: Boolean) {
    val background = if (night) 0xFF2C2B29.toInt() else 0xFFFBFBFB.toInt()
    val text = if (night) 0xFFFBFBFB.toInt() else 0xFF0F0F0F.toInt()
    val accent = 0xFFDA5893.toInt()
    val recording = if (night) 0xFFF87171.toInt() else 0xFFDC2626.toInt()
    fun ink(alpha: Int) = Color.argb(alpha, Color.red(text), Color.green(text), Color.blue(text))
  }

  private fun dp(value: Int) = (value * resources.displayMetrics.density + 0.5f).toInt()

  private fun round(color: Int, radius: Int? = null) = GradientDrawable().apply {
    if (radius == null) shape = GradientDrawable.OVAL else cornerRadius = dp(radius).toFloat()
    setColor(color)
  }

  private fun key(icon: Int, label: Int, onTap: () -> Unit) = ImageButton(this).apply {
    setImageResource(icon)
    imageTintList = ColorStateList.valueOf(palette.ink(0xB3))
    background = round(palette.ink(0x14), 12)
    contentDescription = getString(label)
    setOnClickListener {
      it.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
      onTap()
    }
  }

  @SuppressLint("ClickableViewAccessibility")
  override fun onCreateInputView(): View {
    val night = resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK ==
      Configuration.UI_MODE_NIGHT_YES
    palette = Palette(night)

    val root = LinearLayout(this).apply {
      orientation = LinearLayout.VERTICAL
      setBackgroundColor(palette.background)
      setPadding(0, 0, 0, dp(8))
    }
    // Room for the gesture bar where the keyboard is drawn behind it.
    root.setOnApplyWindowInsetsListener { view, insets ->
      val bottom = if (Build.VERSION.SDK_INT >= 30) {
        insets.getInsets(android.view.WindowInsets.Type.systemBars()).bottom
      } else {
        @Suppress("DEPRECATION") insets.systemWindowInsetBottom
      }
      view.setPadding(0, 0, 0, dp(8) + bottom)
      insets
    }

    // A hairline, so the keyboard doesn't run into a page of the same colour.
    val edge = View(this).apply { setBackgroundColor(palette.ink(0x1F)) }
    root.addView(edge, LinearLayout.LayoutParams(-1, maxOf(1, dp(1) / 2)))

    status = TextView(this).apply {
      textSize = 15f
      setTextColor(palette.ink(0xB3))
      gravity = Gravity.CENTER
      maxLines = 2
      accessibilityLiveRegion = View.ACCESSIBILITY_LIVE_REGION_POLITE
    }
    root.addView(
      status,
      LinearLayout.LayoutParams(-1, dp(40)).apply {
        topMargin = dp(10)
        marginStart = dp(8)
        marginEnd = dp(8)
      },
    )

    val stage = FrameLayout(this)
    halo = View(this).apply {
      background = round(palette.accent)
      alpha = 0.3f
      visibility = View.INVISIBLE
    }
    stage.addView(halo, FrameLayout.LayoutParams(dp(84), dp(84), Gravity.CENTER))
    mic = ImageButton(this).apply {
      setImageResource(R.drawable.ic_keyboard_mic)
      imageTintList = ColorStateList.valueOf(Color.WHITE)
      scaleType = android.widget.ImageView.ScaleType.CENTER_INSIDE
      setPadding(dp(24), dp(24), dp(24), dp(24))
      elevation = dp(3).toFloat()
      setOnClickListener {
        it.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
        onMic()
      }
    }
    stage.addView(mic, FrameLayout.LayoutParams(dp(84), dp(84), Gravity.CENTER))
    spinner = ProgressBar(this).apply {
      indeterminateTintList = ColorStateList.valueOf(Color.WHITE)
      elevation = dp(4).toFloat()
      visibility = View.GONE
    }
    stage.addView(spinner, FrameLayout.LayoutParams(dp(40), dp(40), Gravity.CENTER))
    cancel = key(R.drawable.ic_keyboard_close, R.string.keyboard_cancel) { cancelDictation() }
    stage.addView(
      cancel,
      FrameLayout.LayoutParams(dp(48), dp(48), Gravity.CENTER_VERTICAL or Gravity.END).apply {
        marginEnd = dp(28)
      },
    )
    root.addView(stage, LinearLayout.LayoutParams(-1, dp(124)))

    val row = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
    val switch = key(R.drawable.ic_keyboard_keyboard, R.string.keyboard_switch) { switchKeyboard() }
    switch.setOnLongClickListener {
      getSystemService(InputMethodManager::class.java).showInputMethodPicker()
      true
    }
    val space = TextView(this).apply {
      text = getString(R.string.keyboard_space)
      textSize = 14f
      gravity = Gravity.CENTER
      setTextColor(palette.ink(0x80))
      background = round(palette.ink(0x14), 12)
      setOnClickListener {
        it.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
        currentInputConnection?.commitText(" ", 1)
      }
    }
    val backspace = key(R.drawable.ic_keyboard_backspace, R.string.keyboard_backspace) {}
    // Deletes on touch, then keeps deleting while held.
    val repeat = object : Runnable {
      override fun run() {
        deleteBackward()
        main.postDelayed(this, 50)
      }
    }
    backspace.setOnTouchListener { view, event ->
      when (event.actionMasked) {
        MotionEvent.ACTION_DOWN -> {
          view.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
          view.isPressed = true
          deleteBackward()
          main.postDelayed(repeat, 400)
        }
        MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
          view.isPressed = false
          main.removeCallbacks(repeat)
        }
      }
      true
    }
    val enter = key(R.drawable.ic_keyboard_enter, R.string.keyboard_enter) { pressEnter() }

    val gap = dp(6)
    row.addView(switch, LinearLayout.LayoutParams(dp(52), dp(48)))
    row.addView(space, LinearLayout.LayoutParams(0, dp(48), 1f).apply { marginStart = gap })
    row.addView(backspace, LinearLayout.LayoutParams(dp(52), dp(48)).apply { marginStart = gap })
    row.addView(enter, LinearLayout.LayoutParams(dp(52), dp(48)).apply { marginStart = gap })
    root.addView(
      row,
      LinearLayout.LayoutParams(-1, -2).apply {
        marginStart = dp(8)
        marginEnd = dp(8)
      },
    )

    render()
    return root
  }

  /** Draws the current state. Everything that changes it calls this. */
  private fun render() {
    val mic = mic ?: return
    val name = computer ?: getString(R.string.keyboard_your_computer)
    val microphoneAllowed = microphoneAllowed()

    status?.text = notice ?: when {
      phase == Phase.RECORDING -> getString(R.string.keyboard_listening)
      phase == Phase.TRANSCRIBING -> getString(R.string.keyboard_transcribing)
      linkState == "unpaired" -> getString(R.string.keyboard_unpaired)
      linkState == "rejected" -> getString(R.string.keyboard_rejected)
      linkState == "incompatible" -> getString(R.string.keyboard_incompatible)
      !microphoneAllowed -> getString(R.string.keyboard_needs_microphone)
      linkState == "connecting" -> getString(R.string.keyboard_connecting, name)
      linkState == "offline" -> getString(R.string.keyboard_offline, name)
      else -> getString(R.string.keyboard_tap_to_dictate)
    }

    val recording = phase == Phase.RECORDING
    val transcribing = phase == Phase.TRANSCRIBING
    val ready = linkState == "connected" && microphoneAllowed
    mic.background = round(if (recording) palette.recording else palette.accent)
    mic.setImageResource(if (recording) R.drawable.ic_keyboard_stop else R.drawable.ic_keyboard_mic)
    mic.imageAlpha = if (transcribing) 0 else 255
    mic.alpha = if (ready || phase != Phase.IDLE) 1f else 0.4f
    mic.isEnabled = !transcribing
    mic.contentDescription =
      getString(if (recording) R.string.keyboard_finish else R.string.keyboard_dictate)
    spinner?.visibility = if (transcribing) View.VISIBLE else View.GONE
    cancel?.visibility = if (phase == Phase.IDLE) View.INVISIBLE else View.VISIBLE
    if (!recording) {
      halo?.visibility = View.INVISIBLE
    }
  }

  private fun say(message: Int) {
    notice = getString(message)
    main.removeCallbacks(clearNotice)
    main.postDelayed(clearNotice, 3500)
    render()
  }

  // Lifecycle

  override fun onWindowShown() {
    super.onWindowShown()
    setOnScreen(true)
    link.listener = ::onLinkEvent
    linkState = "connecting"
    // Where the app keeps the pairing (`desktop_path` in src-tauri/src/lib.rs).
    link.open(dataDir.resolve("desktop.json").absolutePath)
    render()
  }

  override fun onWindowHidden() {
    super.onWindowHidden()
    setOnScreen(false)
    cancelDictation()
    link.close()
    notice = null
  }

  override fun onDestroy() {
    recorder?.stop(send = false)
    recorder = null
    link.close()
    link.listener = null
    setOnScreen(false)
    super.onDestroy()
  }

  // Android hides on-screen keyboards while a physical one is attached. This
  // one isn't a substitute for it, so it shows anyway.
  override fun onEvaluateInputViewShown(): Boolean {
    super.onEvaluateInputViewShown()
    return true
  }

  // A dictation keyboard has nothing to show full screen in landscape.
  override fun onEvaluateFullscreenMode() = false

  // Dictation

  private fun microphoneAllowed() =
    checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED

  private fun onMic() {
    when {
      phase == Phase.RECORDING -> finishDictation()
      phase == Phase.TRANSCRIBING -> {}
      linkState == "unpaired" || linkState == "rejected" || linkState == "incompatible" -> openApp()
      // A keyboard can't ask for a permission; the app can.
      !microphoneAllowed() -> openApp()
      linkState != "connected" -> link.retry()
      else -> startDictation()
    }
  }

  private fun startDictation() {
    val started = Recorder(link) { level -> main.post { showLevel(level) } }
    if (!started.start()) {
      say(R.string.keyboard_microphone_failed)
      return
    }
    main.removeCallbacks(clearNotice)
    notice = null
    recorder = started
    phase = Phase.RECORDING
    halo?.visibility = View.VISIBLE
    render()
  }

  private fun finishDictation() {
    // The recorder sends what it still holds, then tells the computer to
    // transcribe.
    recorder?.stop(send = true)
    recorder = null
    phase = Phase.TRANSCRIBING
    render()
  }

  private fun cancelDictation() {
    if (phase == Phase.IDLE) return
    recorder?.stop(send = false)
    recorder = null
    link.cancel()
    phase = Phase.IDLE
    render()
  }

  private fun showLevel(level: Float) {
    if (phase != Phase.RECORDING) return
    val scale = 1f + min(1f, level * 6f) * 0.45f
    halo?.animate()?.scaleX(scale)?.scaleY(scale)?.setDuration(90)?.start()
  }

  private fun onLinkEvent(event: JSONObject) {
    when (event.optString("type")) {
      "link" -> {
        linkState = event.optString("state")
        if (!event.isNull("name")) computer = event.optString("name")
        if (linkState != "connected" && phase != Phase.IDLE) {
          // What was said went down with the connection.
          recorder?.stop(send = false)
          recorder = null
          phase = Phase.IDLE
          say(R.string.keyboard_connection_lost)
        }
        render()
      }
      "final" -> {
        if (phase != Phase.TRANSCRIBING) return
        phase = Phase.IDLE
        val text = event.optString("text").trim()
        if (text.isEmpty()) say(R.string.keyboard_nothing_heard) else type(text)
        render()
      }
      "error" -> {
        val code = event.optString("code")
        if (code == "too_long" && phase == Phase.RECORDING) {
          // The computer has stopped listening and is transcribing what it
          // got; the text follows.
          recorder?.stop(send = false)
          recorder = null
          phase = Phase.TRANSCRIBING
          say(R.string.keyboard_too_long)
          return
        }
        if (phase == Phase.IDLE) return
        recorder?.stop(send = false)
        recorder = null
        phase = Phase.IDLE
        say(
          when (code) {
            "busy" -> R.string.keyboard_busy
            "no_model" -> R.string.keyboard_no_model
            "too_long" -> R.string.keyboard_too_long
            else -> R.string.keyboard_failed
          },
        )
      }
    }
  }

  // Typing

  /** Types a transcript at the cursor, set off from the word before it. */
  private fun type(text: String) {
    val input = currentInputConnection ?: return
    val before = input.getTextBeforeCursor(1, 0)
    val joins = !before.isNullOrEmpty() && !before[0].isWhitespace() &&
      before[0] !in "([{\"'" && text[0] !in ".,!?;:)"
    input.commitText(if (joins) " $text" else text, 1)
  }

  private fun deleteBackward() {
    val input = currentInputConnection ?: return
    if (!input.getSelectedText(0).isNullOrEmpty()) {
      input.commitText("", 1)
    } else {
      // A key press, so the app deletes what it considers one character.
      sendDownUpKeyEvents(KeyEvent.KEYCODE_DEL)
    }
  }

  private fun pressEnter() {
    val info = currentInputEditorInfo
    val action = (info?.imeOptions ?: 0) and EditorInfo.IME_MASK_ACTION
    val wantsNewline = info == null ||
      info.imeOptions and EditorInfo.IME_FLAG_NO_ENTER_ACTION != 0 ||
      action == EditorInfo.IME_ACTION_NONE || action == EditorInfo.IME_ACTION_UNSPECIFIED
    if (wantsNewline) {
      sendDownUpKeyEvents(KeyEvent.KEYCODE_ENTER)
    } else {
      currentInputConnection?.performEditorAction(action)
    }
  }

  private fun switchKeyboard() {
    val switched = Build.VERSION.SDK_INT >= 28 && switchToPreviousInputMethod()
    if (!switched) {
      getSystemService(InputMethodManager::class.java).showInputMethodPicker()
    }
  }

  private fun openApp() {
    startActivity(
      Intent(this, MainActivity::class.java)
        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        .putExtra(MainActivity.EXTRA_KEYBOARD_SETUP, true),
    )
  }

  private fun setOnScreen(shown: Boolean) {
    if (onScreen == shown) return
    onScreen = shown
    onScreenChanged?.invoke()
  }

  companion object {
    /** Whether this keyboard is the one up. The bubble stays away while it is. */
    @Volatile var onScreen = false
      private set
    var onScreenChanged: (() -> Unit)? = null
  }
}

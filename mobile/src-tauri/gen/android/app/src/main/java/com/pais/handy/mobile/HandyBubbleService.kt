package com.pais.handy.mobile

import android.Manifest
import android.accessibilityservice.AccessibilityService
import android.annotation.SuppressLint
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.graphics.PixelFormat
import android.graphics.Rect
import android.graphics.drawable.GradientDrawable
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.view.Choreographer
import android.view.Gravity
import android.view.HapticFeedbackConstants
import android.view.MotionEvent
import android.view.VelocityTracker
import android.view.View
import android.view.ViewConfiguration
import android.view.WindowManager
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo
import android.view.accessibility.AccessibilityWindowInfo
import android.widget.TextView
import org.json.JSONObject
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.min

/**
 * The floating bubble: a microphone at the side of the screen whenever a
 * text field is in use, whatever the keyboard. Tap it to dictate and tap
 * again to finish, or hold it and talk. What it hears is transcribed on the
 * paired computer and typed into the field.
 *
 * It is an accessibility service because only one can know that a text field
 * has focus, draw over other apps without a further permission, and type
 * into them. It reads nothing else from the screen.
 *
 * The body is a [Blob]. It is drawn by two windows that touches pass
 * through: a small one where it rests, and a full-screen one that exists
 * only while it is away from the wall. A third, the size of the ball, takes
 * the touches.
 */
class HandyBubbleService : AccessibilityService() {
  private enum class Phase { IDLE, RECORDING, TRANSCRIBING }

  private val main = Handler(Looper.getMainLooper())
  private val link = PhoneLink.bubble
  private var linkOpen = false
  private var linkState = "connecting"
  private var computer: String? = null
  private var phase = Phase.IDLE
  /** Recording only for as long as the finger stays down. */
  private var holding = false
  private var recorder: Recorder? = null

  private lateinit var overlays: WindowManager
  private lateinit var blob: Blob
  private var hit: View? = null
  private var dock: BlobView? = null
  private var stage: BlobView? = null
  private var stageInUse = false
  private var label: TextView? = null
  /** On screen, or on its way there. */
  private var shown = false
  private var animating = false
  private var lastFrame = 0L

  private fun dp(value: Float) = value * resources.displayMetrics.density

  // Lifecycle

  override fun onServiceConnected() {
    overlays = getSystemService(WindowManager::class.java)
    blob = Blob(this)
    blob.onSnap = { hit?.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK) }
    blob.onAttach = { hit?.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP) }
    measure()
    val saved = getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
    blob.place(
      saved.getInt("side", 1),
      clampY(saved.getFloat("y", 0.42f) * blob.screenHeight),
    )
    link.listener = ::onLinkEvent
    HandyKeyboardService.onScreenChanged = { main.post(::checkFocus) }
    checkFocus()
  }

  override fun onAccessibilityEvent(event: AccessibilityEvent?) {
    // Focus settles a moment after the events that announce it.
    main.removeCallbacks(checkFocusLater)
    main.postDelayed(checkFocusLater, 120)
  }

  override fun onInterrupt() {}

  override fun onConfigurationChanged(newConfig: Configuration) {
    super.onConfigurationChanged(newConfig)
    if (!::blob.isInitialized) return
    val along = blob.dockY / max(1f, blob.screenHeight)
    measure()
    blob.dockY = clampY(along * blob.screenHeight)
    if (!blob.dragging) blob.place(blob.side, blob.dockY)
    if (shown) {
      placeWindows()
      animate()
    }
  }

  override fun onUnbind(intent: Intent?): Boolean {
    shutDown()
    return super.onUnbind(intent)
  }

  override fun onDestroy() {
    shutDown()
    super.onDestroy()
  }

  private fun shutDown() {
    main.removeCallbacksAndMessages(null)
    recorder?.stop(send = false)
    recorder = null
    phase = Phase.IDLE
    link.close()
    linkOpen = false
    link.listener = null
    HandyKeyboardService.onScreenChanged = null
    shown = false
    removeWindows()
  }

  // When to show

  private val checkFocusLater = Runnable { checkFocus() }

  private fun checkFocus() {
    val wanted = textFieldInUse()
    // Something being said is seen through, wherever the focus went.
    if (wanted || phase != Phase.IDLE || blob.dragging || dragPending) show() else hide()
  }

  private fun textFieldInUse(): Boolean {
    // Handy's own keyboard has its own microphone.
    if (HandyKeyboardService.onScreen) return false
    return try {
      val focused = findFocus(AccessibilityNodeInfo.FOCUS_INPUT)
      // Nothing dictated should be seen going into a password.
      if (focused?.isPassword == true) return false
      if (focused?.isEditable == true) return true
      // Some apps draw their own text fields and describe none of them; a
      // keyboard on screen says as much.
      getWindows().any { it.type == AccessibilityWindowInfo.TYPE_INPUT_METHOD }
    } catch (_: Exception) {
      false
    }
  }

  private fun show() {
    main.removeCallbacks(closeLink)
    if (!linkOpen) {
      linkOpen = true
      linkState = "connecting"
      // Where the app keeps the pairing (`desktop_path` in src-tauri/src/lib.rs).
      link.open(dataDir.resolve("desktop.json").absolutePath)
    }
    if (shown) return
    shown = true
    blob.hiding = false
    addWindows()
    render()
    animate()
  }

  private fun hide() {
    if (!shown) return
    shown = false
    blob.hiding = true
    hideLabel()
    animate()
    // Text fields come and go; the connection waits a while for the next.
    main.postDelayed(closeLink, 20_000)
  }

  private val closeLink = Runnable {
    if (shown || phase != Phase.IDLE) return@Runnable
    link.close()
    linkOpen = false
  }

  // Windows

  private fun measure() {
    if (Build.VERSION.SDK_INT >= 30) {
      val bounds = overlays.currentWindowMetrics.bounds
      blob.screenWidth = bounds.width().toFloat()
      blob.screenHeight = bounds.height().toFloat()
    } else {
      val size = android.graphics.Point()
      @Suppress("DEPRECATION") overlays.defaultDisplay.getRealSize(size)
      blob.screenWidth = size.x.toFloat()
      blob.screenHeight = size.y.toFloat()
    }
  }

  /** Keeps the resting place clear of the status and navigation bars. */
  private fun clampY(y: Float) =
    min(max(y, dp(72f) + blob.baseRadius), blob.screenHeight - dp(88f) - blob.baseRadius)

  private fun params(width: Int, height: Int, touchable: Boolean) =
    WindowManager.LayoutParams(
      width,
      height,
      WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY,
      WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
        WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
        WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS or
        (if (touchable) 0 else WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE),
      PixelFormat.TRANSLUCENT,
    ).apply {
      gravity = Gravity.TOP or Gravity.LEFT
      if (Build.VERSION.SDK_INT >= 30) {
        fitInsetsTypes = 0
        layoutInDisplayCutoutMode =
          WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS
      } else if (Build.VERSION.SDK_INT >= 28) {
        layoutInDisplayCutoutMode =
          WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
      }
    }

  /** A window against the bubble's wall, centred on where it rests. */
  private fun docked(width: Float, height: Float, touchable: Boolean) =
    params(width.toInt(), height.toInt(), touchable).apply {
      x = if (blob.side < 0) 0 else (blob.screenWidth - width).toInt()
      y = (blob.dockY - height / 2f).toInt()
    }

  private fun hitParams() =
    docked(blob.baseRadius * 1.8f + dp(8f), blob.baseRadius * 2f + dp(16f), true)

  private fun dockParams() = docked(blob.baseRadius * 3.6f, blob.baseRadius * 6.4f, false)

  @SuppressLint("ClickableViewAccessibility")
  private fun addWindows() {
    if (hit != null) return
    try {
      val dock = BlobView(this, blob)
      // Two windows never show the same frame at the same moment, so only
      // one draws: the full-screen one, once it is up.
      dock.covered = { stageInUse && (stage?.frames ?: 0) >= 2 }
      overlays.addView(dock, dockParams())
      this.dock = dock
      val hit = View(this).apply {
        contentDescription = getString(R.string.bubble_dictate)
        setOnTouchListener { _, event -> onTouch(event) }
        setOnClickListener { onTap() }
        // At the edge of the screen a drag is Android's "back". Not here.
        if (Build.VERSION.SDK_INT >= 29) {
          addOnLayoutChangeListener { view, _, _, _, _, _, _, _, _ ->
            view.systemGestureExclusionRects = listOf(Rect(0, 0, view.width, view.height))
          }
        }
      }
      overlays.addView(hit, hitParams())
      this.hit = hit
    } catch (_: Exception) {
      // The service is on its way out; there is nowhere to draw.
      removeWindows()
    }
  }

  private fun placeWindows() {
    try {
      hit?.let { overlays.updateViewLayout(it, hitParams()) }
      dock?.let { overlays.updateViewLayout(it, dockParams()) }
    } catch (_: Exception) {
    }
  }

  private fun addStage() {
    main.removeCallbacks(removeStageLater)
    stageInUse = true
    if (stage != null) return
    try {
      val stage = BlobView(this, blob)
      overlays.addView(stage, params(-1, -1, false))
      this.stage = stage
    } catch (_: Exception) {
    }
  }

  private fun remove(view: View?) {
    try {
      if (view != null) overlays.removeViewImmediate(view)
    } catch (_: Exception) {
    }
  }

  private fun removeStage() {
    main.removeCallbacks(removeStageLater)
    stageInUse = false
    remove(stage)
    stage = null
  }

  /**
   * Hands the blob back to the small window. The full-screen one stays a
   * moment longer, so there is no frame with neither.
   */
  private fun retireStage() {
    if (!stageInUse) return
    stageInUse = false
    dock?.invalidate()
    main.postDelayed(removeStageLater, 100)
  }

  private val removeStageLater = Runnable { if (!stageInUse) removeStage() }

  private fun removeWindows() {
    hideLabel()
    removeStage()
    remove(hit)
    hit = null
    remove(dock)
    dock = null
  }

  // Motion

  private val frame = object : Choreographer.FrameCallback {
    override fun doFrame(now: Long) {
      val dt = if (lastFrame == 0L) 1f / 60f else min((now - lastFrame) / 1e9f, 1f / 20f)
      lastFrame = now
      if (dragPending && (stage?.frames ?: 2) >= 2) {
        dragPending = false
        blob.dragging = true
      }
      blob.step(dt)
      dock?.invalidate()
      stage?.invalidate()

      val busy = phase != Phase.IDLE
      if (blob.settled && !busy && !dragPending) {
        animating = false
        lastFrame = 0L
        // Back on its wall, where the small window covers it.
        retireStage()
        if (!shown) removeWindows()
        return
      }
      Choreographer.getInstance().postFrameCallback(this)
    }
  }

  private fun animate() {
    if (animating) return
    animating = true
    lastFrame = 0L
    Choreographer.getInstance().postFrameCallback(frame)
  }

  // Touch

  private var downX = 0f
  private var downY = 0f
  private var grabX = 0f
  private var grabY = 0f
  private var longPressed = false
  /**
   * A drag has begun, but the blob holds still until the full-screen window
   * is up to draw it: two windows showing it mid-move would show it twice.
   */
  private var dragPending = false
  private var velocity: VelocityTracker? = null
  private val longPress = Runnable { onLongPress() }

  private fun onTouch(event: MotionEvent): Boolean {
    when (event.actionMasked) {
      MotionEvent.ACTION_DOWN -> {
        downX = event.rawX
        downY = event.rawY
        grabX = blob.x - downX
        grabY = blob.y - downY
        longPressed = false
        velocity?.recycle()
        velocity = VelocityTracker.obtain()
        blob.pressed = true
        main.postDelayed(longPress, LONG_PRESS_MS)
        animate()
      }
      MotionEvent.ACTION_MOVE -> {
        // The tracker wants screen coordinates: the window moves with the drag.
        val moved = MotionEvent.obtain(event)
        moved.setLocation(event.rawX, event.rawY)
        velocity?.addMovement(moved)
        moved.recycle()
        val slop = ViewConfiguration.get(this).scaledTouchSlop
        if (!blob.dragging && !dragPending && !holding && !longPressed &&
          hypot(event.rawX - downX, event.rawY - downY) > slop
        ) {
          main.removeCallbacks(longPress)
          blob.pressed = false
          dragPending = true
          hideLabel()
          addStage()
        }
        if (blob.dragging || dragPending) {
          blob.fingerX = event.rawX + grabX
          blob.fingerY = event.rawY + grabY
          animate()
        }
      }
      MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
        main.removeCallbacks(longPress)
        blob.pressed = false
        val cancelled = event.actionMasked == MotionEvent.ACTION_CANCEL
        if (dragPending) {
          dragPending = false
          blob.dragging = true
        }
        when {
          blob.dragging -> drop()
          holding -> {
            holding = false
            if (cancelled) cancelDictation() else finishDictation()
          }
          !longPressed && !cancelled -> hit?.performClick()
        }
        velocity?.recycle()
        velocity = null
        animate()
      }
    }
    return true
  }

  /** Lets the bubble go: it makes for the nearer wall, carried by the throw. */
  private fun drop() {
    val tracker = velocity
    tracker?.computeCurrentVelocity(1000)
    val speedX = tracker?.xVelocity ?: 0f
    val speedY = tracker?.yVelocity ?: 0f
    val side = if (blob.x + speedX * 0.15f < blob.screenWidth / 2f) -1 else 1
    blob.release(side, clampY(blob.y + speedY * 0.12f), speedX, speedY)
    placeWindows()
    getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE).edit()
      .putInt("side", blob.side)
      .putFloat("y", blob.dockY / max(1f, blob.screenHeight))
      .apply()
    checkFocus()
  }

  private fun unavailable() =
    linkState == "unpaired" || linkState == "rejected" || linkState == "incompatible" ||
      !microphoneAllowed()

  private fun onTap() {
    hit?.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
    when {
      phase == Phase.RECORDING -> finishDictation()
      phase == Phase.TRANSCRIBING -> {
        cancelDictation()
        say(getString(R.string.bubble_cancelled))
      }
      else -> startOrExplain(hold = false)
    }
  }

  private fun onLongPress() {
    longPressed = true
    hit?.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
    when (phase) {
      // A way out of a dictation started with a tap.
      Phase.RECORDING, Phase.TRANSCRIBING -> {
        cancelDictation()
        say(getString(R.string.bubble_cancelled))
      }
      Phase.IDLE -> startOrExplain(hold = true)
    }
  }

  private fun startOrExplain(hold: Boolean) {
    val name = computer ?: getString(R.string.keyboard_your_computer)
    when {
      // A service can't ask for a permission or pair; the app can.
      unavailable() -> openApp()
      linkState == "connected" -> startDictation(hold)
      linkState == "connecting" -> say(getString(R.string.keyboard_connecting, name))
      else -> {
        link.retry()
        say(getString(R.string.bubble_offline, name))
      }
    }
  }

  // Dictation

  private fun microphoneAllowed() =
    checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED

  private fun startDictation(hold: Boolean) {
    val started = Recorder(link) { level -> main.post { blob.level = min(1f, level * 6f) } }
    if (!started.start()) {
      say(getString(R.string.keyboard_microphone_failed))
      return
    }
    hideLabel()
    recorder = started
    holding = hold
    phase = Phase.RECORDING
    render()
  }

  private fun finishDictation() {
    if (phase != Phase.RECORDING) return
    // The recorder sends what it still holds, then tells the computer to
    // transcribe.
    recorder?.stop(send = true)
    recorder = null
    holding = false
    phase = Phase.TRANSCRIBING
    render()
  }

  private fun cancelDictation() {
    if (phase == Phase.IDLE) return
    recorder?.stop(send = false)
    recorder = null
    link.cancel()
    endDictation()
  }

  private fun endDictation() {
    recorder?.stop(send = false)
    recorder = null
    holding = false
    phase = Phase.IDLE
    render()
    checkFocus()
  }

  private fun onLinkEvent(event: JSONObject) {
    when (event.optString("type")) {
      "link" -> {
        linkState = event.optString("state")
        if (!event.isNull("name")) computer = event.optString("name")
        if (linkState != "connected" && phase != Phase.IDLE) {
          // What was said went down with the connection.
          endDictation()
          say(getString(R.string.keyboard_connection_lost))
        }
        render()
      }
      "final" -> {
        if (phase != Phase.TRANSCRIBING) return
        val text = event.optString("text").trim()
        when {
          text.isEmpty() -> say(getString(R.string.keyboard_nothing_heard))
          !type(text) -> {
            getSystemService(ClipboardManager::class.java)
              .setPrimaryClip(ClipData.newPlainText(getString(R.string.app_name), text))
            say(getString(R.string.bubble_copied))
          }
        }
        endDictation()
      }
      "error" -> {
        val code = event.optString("code")
        if (code == "too_long" && phase == Phase.RECORDING) {
          // The computer has stopped listening and is transcribing what it
          // got; the text follows.
          recorder?.stop(send = false)
          recorder = null
          holding = false
          phase = Phase.TRANSCRIBING
          say(getString(R.string.keyboard_too_long))
          render()
          return
        }
        if (phase == Phase.IDLE) return
        endDictation()
        say(
          getString(
            when (code) {
              "busy" -> R.string.keyboard_busy
              "no_model" -> R.string.keyboard_no_model
              "too_long" -> R.string.keyboard_too_long
              else -> R.string.keyboard_failed
            },
          ),
        )
      }
    }
  }

  /** Shows the current state on the bubble. Everything that changes it calls this. */
  private fun render() {
    blob.look = when {
      phase == Phase.RECORDING -> if (holding) Blob.Look.HOLDING else Blob.Look.RECORDING
      phase == Phase.TRANSCRIBING -> Blob.Look.BUSY
      linkState == "connected" && microphoneAllowed() -> Blob.Look.IDLE
      else -> Blob.Look.UNAVAILABLE
    }
    if (phase != Phase.RECORDING) blob.level = 0f
    hit?.contentDescription = getString(
      if (phase == Phase.RECORDING) R.string.keyboard_finish else R.string.bubble_dictate,
    )
    if (shown) animate()
  }

  // Typing

  /** Sets a transcript off from the character before it. */
  private fun spaced(text: String, before: Char?) =
    if (before != null && !before.isWhitespace() && before !in "([{\"'" && text[0] !in ".,!?;:)") {
      " $text"
    } else {
      text
    }

  /** Types a transcript into the focused text field. False if it can't. */
  private fun type(text: String): Boolean {
    try {
      // The way a keyboard types, where Android offers it to a service.
      if (Build.VERSION.SDK_INT >= 33) {
        val input = inputMethod?.currentInputConnection
        if (input != null) {
          val around = input.getSurroundingText(1, 0, 0)
          val before = around?.let {
            val at = it.selectionStart
            if (at in 1..it.text.length) it.text[at - 1] else null
          }
          input.commitText(spaced(text, before), 1, null)
          return true
        }
      }

      val field = findFocus(AccessibilityNodeInfo.FOCUS_INPUT) ?: return false
      if (!field.isEditable) return false
      // An empty field reports its placeholder as its text.
      val current = if (field.isShowingHintText) "" else field.text?.toString().orEmpty()
      var start = min(field.textSelectionStart, field.textSelectionEnd)
      var end = max(field.textSelectionStart, field.textSelectionEnd)
      if (start < 0 || end > current.length) {
        start = current.length
        end = current.length
      }
      val typed = spaced(text, current.getOrNull(start - 1))
      val replaced = field.performAction(
        AccessibilityNodeInfo.ACTION_SET_TEXT,
        Bundle().apply {
          putCharSequence(
            AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE,
            current.substring(0, start) + typed + current.substring(end),
          )
        },
      )
      if (!replaced) return false
      val cursor = start + typed.length
      field.performAction(
        AccessibilityNodeInfo.ACTION_SET_SELECTION,
        Bundle().apply {
          putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_START_INT, cursor)
          putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_END_INT, cursor)
        },
      )
      return true
    } catch (_: Exception) {
      return false
    }
  }

  private fun openApp() {
    startActivity(
      Intent(this, MainActivity::class.java)
        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        .putExtra(MainActivity.EXTRA_KEYBOARD_SETUP, true),
    )
  }

  // The label: a line of text beside the bubble, for what it can't show.

  private val hideLabelLater = Runnable { hideLabel() }

  private fun say(message: String) {
    if (!shown || blob.dragging || dragPending) return
    hideLabel()
    val text = TextView(this).apply {
      text = message
      textSize = 14f
      setTextColor(0xFFFBFBFB.toInt())
      maxLines = 3
      setPadding(dp(14f).toInt(), dp(9f).toInt(), dp(14f).toInt(), dp(9f).toInt())
      background = GradientDrawable().apply {
        cornerRadius = dp(18f)
        setColor(0xF02C2B29.toInt())
      }
      accessibilityLiveRegion = View.ACCESSIBILITY_LIVE_REGION_POLITE
    }
    val beside = (blob.baseRadius * 2.1f + dp(8f)).toInt()
    text.maxWidth = (blob.screenWidth - beside - dp(16f)).toInt()
    val where = params(-2, -2, false).apply {
      gravity = Gravity.TOP or (if (blob.side < 0) Gravity.LEFT else Gravity.RIGHT)
      x = beside
      y = (blob.dockY - dp(19f)).toInt()
    }
    try {
      overlays.addView(text, where)
      label = text
      main.postDelayed(hideLabelLater, 3500)
    } catch (_: Exception) {
    }
  }

  private fun hideLabel() {
    main.removeCallbacks(hideLabelLater)
    remove(label)
    label = null
  }

  private companion object {
    const val PREFERENCES = "bubble"
    const val LONG_PRESS_MS = 280L
  }
}

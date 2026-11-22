package com.pais.handy.mobile

import android.animation.ArgbEvaluator
import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import android.graphics.drawable.Drawable
import android.view.View
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.ceil
import kotlin.math.cos
import kotlin.math.exp
import kotlin.math.max
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * The bubble's body: a ball that clings to a side of the screen like a drop
 * of water. Pulled away, the water between them stretches into a neck, thins
 * and lets go; brought back, it merges again.
 *
 * Everything is in screen coordinates. [HandyBubbleService] moves it and
 * decides what it shows; [BlobView] draws it into whichever window covers it.
 *
 * The shape while attached is the ball, the wall, and a fillet rounding off
 * each corner between them: a circle of radius [fillet] touching both. With
 * the ball `d` from the wall, the two fillets meet, and the neck closes, once
 * `fillet` is as small as [pinch] says. That one rule gives both the snap
 * when the ball is dragged off and the merge when it comes back.
 */
class Blob(context: Context) {
  enum class Look { IDLE, UNAVAILABLE, RECORDING, HOLDING, BUSY }

  private val density = context.resources.displayMetrics.density
  val baseRadius = 24f * density
  /** How far from the wall the ball's centre rests, in radii. */
  private val restDepth = 0.8f
  private val restFillet = 0.6f * baseRadius

  var screenWidth = 0f
  var screenHeight = 0f

  /** The wall it belongs to: -1 left, 1 right. */
  var side = 1
  var x = 0f
    private set
  var y = 0f
    private set
  private var vx = 0f
  private var vy = 0f

  /** Where it rests along the wall. */
  var dockY = 0f
  /** Under a finger, at [fingerX], [fingerY]. */
  var dragging = false
  var fingerX = 0f
  var fingerY = 0f
  /** Sinking into the wall, out of sight. */
  var hiding = true

  var attached = true
    private set
  private var fillet = restFillet
  var radius = baseRadius
    private set
  private var radiusSpeed = 0f
  var pressed = false

  // What's left on each side of a neck that just broke.
  private var tail = 0f
  private var tailSide = 1
  private var residue = 0f
  private var residueSide = 1
  private var residueY = 0f

  var look = Look.IDLE
  /** How loud the microphone is, 0 to 1. */
  var level = 0f
  private var shownLevel = 0f
  private var color = ACCENT
  private var spin = 0f

  /** Called as the neck breaks and as it forms. */
  var onSnap: (() -> Unit)? = null
  var onAttach: (() -> Unit)? = null

  private fun wall(side: Int) = if (side < 0) 0f else screenWidth
  private fun depth() = if (side < 0) x else screenWidth - x
  private fun restX(side: Int) = wall(side) - side * restDepth * baseRadius

  /** The fillet radius at which the neck to a ball `d` away closes. */
  private fun pinch(d: Float): Float {
    val reach = d + radius
    return if (reach <= 0f) 0f else reach - sqrt(2f * radius * reach)
  }

  /** Puts it at rest inside the wall, from where it rises when shown. */
  fun place(side: Int, dockY: Float) {
    this.side = side
    this.dockY = dockY
    x = wall(side) + side * 1.6f * baseRadius
    y = dockY
    vx = 0f
    vy = 0f
    attached = true
    fillet = restFillet
    tail = 0f
    residue = 0f
  }

  /** Lets go of it, moving at the finger's speed, to settle on [side]. */
  fun release(side: Int, dockY: Float, speedX: Float, speedY: Float) {
    dragging = false
    if (!attached) this.side = side
    this.dockY = dockY
    vx = speedX
    vy = speedY
  }

  /** Whether it has come to rest where it is headed. */
  var settled = true
    private set

  fun step(dt: Float) {
    val (targetX, targetY) = target()
    // Stiff under the finger; loose on its own, so it sloshes into the wall.
    val stiffness = if (dragging) 1500f else 240f
    val damping = 2f * sqrt(stiffness) * (if (dragging) 0.85f else 0.5f)
    val steps = max(1, ceil(dt / 0.004f).toInt())
    val h = dt / steps
    repeat(steps) {
      // The wall pushes back harder than the air: it lands with a squash,
      // not by sinking out of sight.
      val inWall = !dragging && !hiding && (x - targetX) * side > 0f
      val push = if (inWall) 4f else 1f
      vx += (-stiffness * push * (x - targetX) - damping * (if (inWall) 1.6f else 1f) * vx) * h
      vy += (-stiffness * (y - targetY) - damping * vy) * h
      x += vx * h
      y += vy * h
      val wanted = baseRadius * when {
        pressed -> 0.9f
        look == Look.RECORDING || look == Look.HOLDING -> 1.08f
        else -> 1f
      }
      radiusSpeed += (-520f * (radius - wanted) - 15f * radiusSpeed) * h
      radius += radiusSpeed * h
    }

    if (!attached && dragging) side = if (x < screenWidth / 2f) -1 else 1
    val d = depth()
    if (attached) {
      fillet += (restFillet - fillet) * (1f - exp(-dt * 16f))
      if (pinch(d) > fillet) {
        attached = false
        tail = 1f
        tailSide = side
        residue = 1f
        residueSide = side
        residueY = y
        radiusSpeed += baseRadius * 7f
        onSnap?.invoke()
      }
    } else if (d < radius * 1.45f) {
      attached = true
      // Starts as a thread and widens.
      fillet = max(pinch(d), 0f) + density
      radiusSpeed -= baseRadius * 5f
      onAttach?.invoke()
    }

    val fade = exp(-dt * 13f)
    tail = if (tail < 0.02f) 0f else tail * fade
    residue = if (residue < 0.02f) 0f else residue * fade
    shownLevel += (level - shownLevel) * (1f - exp(-dt * 14f))
    spin = (spin + dt * 400f) % 360f
    val wantedColor = when (look) {
      Look.RECORDING, Look.HOLDING -> RECORDING
      Look.UNAVAILABLE -> UNAVAILABLE
      else -> ACCENT
    }
    if (color != wantedColor) {
      color = ArgbEvaluator().evaluate(1f - exp(-dt * 14f), color, wantedColor) as Int
      if (close(color, wantedColor)) color = wantedColor
    }

    settled = !dragging && tail == 0f && residue == 0f && color == wantedColor &&
      abs(x - targetX) < 0.5f && abs(y - targetY) < 0.5f &&
      abs(vx) < 8f && abs(vy) < 8f &&
      abs(radius - baseRadius * (if (pressed) 0.9f else 1f)) < 0.3f && abs(radiusSpeed) < 4f &&
      abs(fillet - restFillet) < 0.5f
  }

  private fun close(a: Int, b: Int) =
    abs(Color.red(a) - Color.red(b)) + abs(Color.green(a) - Color.green(b)) +
      abs(Color.blue(a) - Color.blue(b)) < 6

  private fun target(): Pair<Float, Float> = when {
    dragging && attached -> {
      // The water holds on: it gives, but less than the finger asks.
      val rest = restDepth * baseRadius
      val asked = if (side < 0) fingerX else screenWidth - fingerX
      val given = rest + (asked - rest) * 0.8f
      Pair(wall(side) - side * given, fingerY)
    }
    dragging -> Pair(fingerX, fingerY)
    hiding -> Pair(wall(side) + side * 1.6f * baseRadius, dockY)
    else -> Pair(restX(side), dockY)
  }

  // Drawing

  private val fill = Paint(Paint.ANTI_ALIAS_FLAG).apply {
    setShadowLayer(6f * density, 0f, 2f * density, 0x40000000)
  }
  private val halo = Paint(Paint.ANTI_ALIAS_FLAG)
  private val stroke = Paint(Paint.ANTI_ALIAS_FLAG).apply {
    style = Paint.Style.STROKE
    strokeWidth = 2.5f * density
    strokeCap = Paint.Cap.ROUND
    color = Color.WHITE
  }
  private val path = Path()
  private val oval = RectF()
  private val mic: Drawable? = context.getDrawable(R.drawable.ic_keyboard_mic)?.mutate()
  private val stop: Drawable? = context.getDrawable(R.drawable.ic_keyboard_stop)?.mutate()

  fun draw(canvas: Canvas) {
    fill.color = color
    val recording = look == Look.RECORDING || look == Look.HOLDING
    if (recording) {
      halo.color = color
      halo.alpha = 0x48
      canvas.drawCircle(x, y, radius * (1.3f + shownLevel * 0.8f), halo)
    }

    if (attached) {
      drawAttached(canvas)
    } else {
      canvas.drawCircle(x, y, radius, fill)
      if (tail > 0f) drawTail(canvas)
    }
    if (residue > 0f) drawResidue(canvas)

    if (look == Look.BUSY) {
      val r = radius * 0.42f
      oval.set(x - r, y - r, x + r, y + r)
      canvas.drawArc(oval, spin, 270f, false, stroke)
    } else {
      val icon = (if (look == Look.RECORDING) stop else mic) ?: return
      val half = (radius * 0.48f).toInt()
      icon.setTint(Color.WHITE)
      icon.setBounds(x.toInt() - half, y.toInt() - half, x.toInt() + half, y.toInt() + half)
      icon.draw(canvas)
    }
  }

  /** Draws in a frame where the wall is the line u = 0 and the ball is at (d, 0). */
  private inline fun inWallFrame(canvas: Canvas, side: Int, atY: Float, body: () -> Unit) {
    canvas.save()
    canvas.translate(wall(side), atY)
    canvas.scale(if (side < 0) 1f else -1f, 1f)
    body()
    canvas.restore()
  }

  private fun drawAttached(canvas: Canvas) {
    val d = depth()
    val r = radius
    if (d <= -r) return
    val f = max(fillet, pinch(d))
    val reach = (r + f) * (r + f) - (d - f) * (d - f)
    if (reach <= 0f) return
    val yf = sqrt(reach)
    // Where a fillet touches the ball, seen from the fillet's centre and
    // from the ball's.
    val fromFillet = Math.toDegrees(atan2(yf, d - f).toDouble()).toFloat()
    val fromBall = Math.toDegrees(atan2(-yf, f - d).toDouble()).toFloat()

    inWallFrame(canvas, side, y) {
      path.rewind()
      path.moveTo(-2f, -yf)
      path.lineTo(0f, -yf)
      oval.set(0f, -yf - f, 2f * f, -yf + f)
      path.arcTo(oval, 180f, fromFillet - 180f)
      oval.set(d - r, -r, d + r, r)
      path.arcTo(oval, fromBall, -2f * fromBall)
      oval.set(0f, yf - f, 2f * f, yf + f)
      path.arcTo(oval, -fromFillet, fromFillet - 180f)
      path.lineTo(-2f, yf)
      path.close()
      canvas.drawPath(path, fill)
    }
  }

  /** The point the ball keeps for a moment where the neck was. */
  private fun drawTail(canvas: Canvas) {
    val d = if (tailSide < 0) x else screenWidth - x
    val r = radius
    val spread = 0.6f
    val baseU = d - r * cos(spread)
    val baseV = r * sin(spread)
    val tip = d - r - 0.5f * r * tail
    inWallFrame(canvas, tailSide, y) {
      path.rewind()
      path.moveTo(baseU, -baseV)
      path.quadTo(d - r * 1.02f, -r * 0.1f, tip, 0f)
      path.quadTo(d - r * 1.02f, r * 0.1f, baseU, baseV)
      path.close()
      canvas.drawPath(path, fill)
    }
  }

  /** The drop left on the wall, sinking back into it. */
  private fun drawResidue(canvas: Canvas) {
    val half = restFillet * (0.5f + 0.5f * residue)
    val tip = restFillet * residue
    inWallFrame(canvas, residueSide, residueY) {
      path.rewind()
      path.moveTo(-2f, -half)
      path.lineTo(0f, -half)
      path.quadTo(tip * 0.15f, -half * 0.15f, tip, 0f)
      path.quadTo(tip * 0.15f, half * 0.15f, 0f, half)
      path.lineTo(-2f, half)
      path.close()
      canvas.drawPath(path, fill)
    }
  }

  private companion object {
    const val ACCENT = 0xFFDA5893.toInt()
    const val RECORDING = 0xFFDC2626.toInt()
    const val UNAVAILABLE = 0xFF8C8C8C.toInt()
  }
}

/** A window's view of the [Blob]: whatever part of it falls inside. */
class BlobView(context: Context, private val blob: Blob) : View(context) {
  private val origin = IntArray(2)
  /** How many times it has drawn. */
  var frames = 0
    private set
  /** Another window has the blob for now, and this one stays empty. */
  var covered: () -> Boolean = { false }

  override fun onDraw(canvas: Canvas) {
    frames++
    if (covered()) return
    getLocationOnScreen(origin)
    canvas.save()
    canvas.translate(-origin[0].toFloat(), -origin[1].toFloat())
    blob.draw(canvas)
    canvas.restore()
  }
}

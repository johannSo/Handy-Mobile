package com.pais.handy.mobile

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.SystemClock
import android.provider.Settings
import android.view.View
import android.view.inputmethod.InputMethodManager
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import org.json.JSONObject

class MainActivity : TauriActivity() {
  /** Set when the keyboard sent the user here to finish setting it up. */
  @Volatile private var setupRequested = false
  private var permissionAskedAt = 0L

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    setupRequested = intent?.getBooleanExtra(EXTRA_KEYBOARD_SETUP, false) == true

    // Edge to edge, Android no longer resizes the window for the keyboard;
    // it slides the whole page up instead, status bar and all. Make room for
    // the keyboard here so the page keeps its place and only gets shorter.
    val content = findViewById<View>(android.R.id.content)
    ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
      val keyboard = insets.getInsets(WindowInsetsCompat.Type.ime())
      view.setPadding(0, 0, 0, keyboard.bottom)
      insets
    }

    requestLocalNetworkAccess()
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    if (intent.getBooleanExtra(EXTRA_KEYBOARD_SETUP, false)) setupRequested = true
  }

  override fun onWebViewCreate(webView: WebView) {
    // The page only ever loads the app's own bundled files.
    webView.addJavascriptInterface(SetupBridge(), "HandySetup")
  }

  // From Android 17 on, an app can't reach anything on the local network
  // until the user allows it: connections just time out, as if the computer
  // were off. Ask up front, since nothing in this app works without it.
  private fun requestLocalNetworkAccess() {
    if (Build.VERSION.SDK_INT < 37) return
    if (checkSelfPermission(LOCAL_NETWORK) != PackageManager.PERMISSION_GRANTED) {
      requestPermissions(arrayOf(LOCAL_NETWORK), REQUEST_AT_LAUNCH)
    }
  }

  private fun granted(permission: String) =
    checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED

  override fun onRequestPermissionsResult(
    requestCode: Int,
    permissions: Array<out String>,
    grantResults: IntArray,
  ) {
    super.onRequestPermissionsResult(requestCode, permissions, grantResults)
    if (requestCode != REQUEST_FROM_SETUP) return
    val denied = grantResults.isEmpty() || grantResults[0] != PackageManager.PERMISSION_GRANTED
    // An answer this fast is Android's, not the user's: it has stopped
    // asking. The app's settings page is the only place left to allow it.
    if (denied && SystemClock.elapsedRealtime() - permissionAskedAt < 500) openAppSettings()
  }

  private fun openAppSettings() {
    startActivity(
      Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:$packageName")),
    )
  }

  /**
   * What the keyboard setup screen (`src/screens/KeyboardSetup.tsx`) needs
   * from Android. The WebView calls these off the main thread.
   */
  private inner class SetupBridge {
    /** Where setup stands, as JSON. */
    @JavascriptInterface
    fun status(): String {
      val methods = getSystemService(InputMethodManager::class.java)
      val enabled = methods.enabledInputMethodList.any { it.packageName == packageName }
      val current = Settings.Secure.getString(contentResolver, Settings.Secure.DEFAULT_INPUT_METHOD)
      return JSONObject()
        .put("microphone", granted(Manifest.permission.RECORD_AUDIO))
        // Before Android 17 the local network needs no permission.
        .put("localNetwork", Build.VERSION.SDK_INT < 37 || granted(LOCAL_NETWORK))
        .put("localNetworkNeeded", Build.VERSION.SDK_INT >= 37)
        .put("keyboardEnabled", enabled)
        .put("keyboardSelected", enabled && current?.startsWith("$packageName/") == true)
        .toString()
    }

    /** True once after the keyboard sent the user here. */
    @JavascriptInterface
    fun takeSetupRequest(): Boolean {
      val requested = setupRequested
      setupRequested = false
      return requested
    }

    /** Asks for "microphone" or "localNetwork". */
    @JavascriptInterface
    fun requestPermission(which: String) {
      val permission = if (which == "microphone") Manifest.permission.RECORD_AUDIO else LOCAL_NETWORK
      runOnUiThread {
        permissionAskedAt = SystemClock.elapsedRealtime()
        requestPermissions(arrayOf(permission), REQUEST_FROM_SETUP)
      }
    }

    /** Opens the system's list of keyboards, where Handy's is turned on. */
    @JavascriptInterface
    fun openKeyboardSettings() {
      runOnUiThread { startActivity(Intent(Settings.ACTION_INPUT_METHOD_SETTINGS)) }
    }

    /** Shows the system's keyboard chooser. */
    @JavascriptInterface
    fun showKeyboardPicker() {
      runOnUiThread { getSystemService(InputMethodManager::class.java).showInputMethodPicker() }
    }
  }

  companion object {
    const val EXTRA_KEYBOARD_SETUP = "keyboard_setup"
    private const val LOCAL_NETWORK = "android.permission.ACCESS_LOCAL_NETWORK"
    private const val REQUEST_AT_LAUNCH = 4701
    private const val REQUEST_FROM_SETUP = 4702
  }
}

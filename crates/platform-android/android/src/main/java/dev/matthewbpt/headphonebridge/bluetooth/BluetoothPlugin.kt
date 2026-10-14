package dev.matthewbpt.headphonebridge.bluetooth

import android.Manifest
import android.app.Activity
import android.os.Handler
import android.os.Looper
import android.webkit.WebView
import androidx.appcompat.app.AppCompatActivity
import androidx.core.app.ActivityCompat
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Plugin

/** How long the app screen keeps holding Devices after it leaves the foreground. */
private const val GRACE_MS = 3000L

/**
 * The app screen's side of Bluetooth. It asks for the Nearby devices
 * permission on first launch, and holds every Connected Device while the
 * screen is in the foreground and for [GRACE_MS] after, so the vendor apps can
 * have the control links otherwise.
 *
 * Tauri calls [load] after its Rust setup, which may already have started the
 * core; [Hub.ensureStarted] copes either way.
 */
@TauriPlugin
class BluetoothPlugin(private val activity: Activity) : Plugin(activity) {
  private val main = Handler(Looper.getMainLooper())
  private val endHold = Runnable { Hub.endHoldAll() }

  override fun load(webView: WebView) {
    Hub.ensureStarted(activity)
    if (!Hub.hasPermission()) {
      ActivityCompat.requestPermissions(activity, arrayOf(Manifest.permission.BLUETOOTH_CONNECT), 1)
    }
    Hub.holdAll()
  }

  override fun onResume(activity: AppCompatActivity) {
    main.removeCallbacks(endHold)
    // Back from the permission prompt, perhaps with it granted.
    Hub.startWatchingIfAllowed()
    Hub.holdAll()
  }

  override fun onPause(activity: AppCompatActivity) {
    main.postDelayed(endHold, GRACE_MS)
  }
}

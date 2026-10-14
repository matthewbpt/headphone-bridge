package dev.matthewbpt.headphonebridge.bluetooth

import android.Manifest
import android.app.Activity
import android.content.ClipData
import android.content.Intent
import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.webkit.WebView
import androidx.appcompat.app.AppCompatActivity
import androidx.core.app.ActivityCompat
import androidx.core.content.FileProvider
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File

/** What the shell's `share_file` sends [BluetoothPlugin.shareFile]. */
@InvokeArg
class ShareFileArgs {
  lateinit var path: String
  lateinit var title: String
}

/** How long the app screen keeps holding Devices after it leaves the foreground. */
private const val GRACE_MS = 3000L

/**
 * The app screen's side of Bluetooth. It asks for the Nearby devices
 * permission on first launch, and holds every Connected Device while the
 * screen is in the foreground and for [GRACE_MS] after, so the vendor apps can
 * have the control links otherwise.
 *
 * Its commands tell the app screen whether the permission is granted, open
 * the app's page in Android's settings (the only way to grant it once
 * refused), and hand files to the share sheet for the shell: see [shareFile].
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

  /** Answers `{"granted": true|false}`: whether the app has the Nearby devices permission. */
  @Command
  fun nearbyDevicesPermission(invoke: Invoke) {
    invoke.resolve(JSObject().put("granted", Hub.hasPermission()))
  }

  /** Opens the app's page in Android's settings, where the listener can grant the permission. */
  @Command
  fun openAppSettings(invoke: Invoke) {
    val intent = Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS)
      .setData(Uri.fromParts("package", activity.packageName, null))
    activity.startActivity(intent)
    invoke.resolve()
  }

  /**
   * Opens the share sheet with a copy of the file at `path`, as plain text,
   * so it can be saved to Files or sent from the phone. Resolves once the
   * sheet is open; rejects with why it couldn't be.
   */
  @Command
  fun shareFile(invoke: Invoke) {
    val args = invoke.parseArgs(ShareFileArgs::class.java)
    try {
      val copy = copyForSharing(File(args.path), activity.cacheDir)
      val uri = FileProvider.getUriForFile(activity, "${activity.packageName}.shared", copy)
      val send =
        Intent(Intent.ACTION_SEND)
          .setType("text/plain")
          .putExtra(Intent.EXTRA_STREAM, uri)
          .putExtra(Intent.EXTRA_SUBJECT, copy.name)
          .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
      // Lets the share sheet preview the file, and grants it to the chosen app.
      send.clipData = ClipData.newRawUri(copy.name, uri)
      activity.startActivity(Intent.createChooser(send, args.title))
      invoke.resolve()
    } catch (e: Exception) {
      invoke.reject(e.message ?: e.toString())
    }
  }
}

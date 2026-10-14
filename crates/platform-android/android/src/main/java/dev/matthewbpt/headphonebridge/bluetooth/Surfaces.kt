package dev.matthewbpt.headphonebridge.bluetooth

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.util.Log
import androidx.core.content.ContextCompat
import androidx.core.content.edit

/**
 * The stored copy and action controller the widget and tile share, one of
 * each per process.
 */
object Surfaces {
  private const val PREFS = "headphone_bridge_surfaces"
  private const val KEY = "copy"

  private var copy: StoredCopy? = null
  private var controller: ActionController? = null
  private var shown: StoredCopy.Shown? = null

  @Synchronized
  fun copy(context: Context): StoredCopy {
    copy?.let { return it }
    val app = context.applicationContext
    val prefs = app.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
    val storage = object : StoredCopy.Storage {
      override fun read(): String? = prefs.getString(KEY, null)
      override fun write(copy: String) = prefs.edit { putString(KEY, copy) }
    }
    return StoredCopy(storage, System::currentTimeMillis) { render(app) }.also { copy = it }
  }

  @Synchronized
  fun controller(context: Context): ActionController {
    controller?.let { return it }
    val app = context.applicationContext
    return ActionController(
      Hub,
      copy(app),
      start = {
        Hub.ensureStarted(app)
        Hub.hasPermission()
      },
      log = { Log.i(TAG, it) },
    ).also { controller = it }
  }

  /**
   * Starts watching Bluetooth for the widget and tile, so they don't show a
   * Device that disconnected while the process was dead. Never Attaches.
   */
  fun watch(context: Context) = controller(context).watch()

  /** Whether the app has the Nearby devices permission, without starting Bluetooth. */
  fun hasPermission(context: Context): Boolean =
    ContextCompat.checkSelfPermission(context, Manifest.permission.BLUETOOTH_CONNECT) ==
      PackageManager.PERMISSION_GRANTED

  /** Shows the copy on every surface, if what they show changed. */
  private fun render(context: Context) {
    val now = copy(context).shown(hasPermission(context))
    synchronized(this) {
      if (now == shown) return
      shown = now
    }
    ModeWidget.updateAll(context)
    ModeTile.update(context)
  }
}

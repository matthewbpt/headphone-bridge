package dev.matthewbpt.headphonebridge.bluetooth

import android.annotation.SuppressLint
import android.app.PendingIntent
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.graphics.drawable.Icon
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import android.util.Log

/**
 * The "Noise Control" Quick Settings tile. Its label and icon are the Primary
 * Device's Chosen Mode and its subtitle the Device's name, or what an action
 * is doing. Its manifest icon stays the headphones, for the Quick Settings
 * editor.
 * Each tap steps to the next Noise Control Mode through the widget's action
 * controller, which works with the phone locked. Long-pressing it opens the
 * app, through [OpenAppActivity].
 */
class ModeTile : TileService() {
  override fun onStartListening() {
    listening = this
    Surfaces.watch(this)
    show()
  }

  override fun onStopListening() {
    if (listening === this) listening = null
  }

  override fun onClick() {
    when (val tap = look().tap) {
      is TileLook.Tap.SetMode -> {
        Log.i(TAG, "tile: set ${tap.mode}")
        Surfaces.controller(this).tap(tap.mode)
      }

      TileLook.Tap.Refresh -> {
        Log.i(TAG, "tile: refresh")
        Surfaces.controller(this).tap(null)
      }

      TileLook.Tap.OpenApp -> {
        if (isLocked) unlockAndRun { openApp() } else openApp()
      }

      TileLook.Tap.Nothing -> {}
    }
    show()
  }

  private fun look() = tileLook(Surfaces.copy(this).shown(hasNearbyDevicesPermission(this)))

  private fun show() {
    val tile = qsTile ?: return
    val look = look()
    tile.label = look.label
    tile.subtitle = look.subtitle
    tile.icon = Icon.createWithResource(this, look.icon)
    tile.state =
      when (look.state) {
        TileLook.State.ACTIVE -> Tile.STATE_ACTIVE
        TileLook.State.INACTIVE -> Tile.STATE_INACTIVE
        TileLook.State.UNAVAILABLE -> Tile.STATE_UNAVAILABLE
      }
    tile.updateTile()
  }

  // Android 12 and 13 have only the Intent overload.
  @SuppressLint("StartActivityAndCollapseDeprecated")
  private fun openApp() {
    val intent = packageManager.getLaunchIntentForPackage(packageName) ?: return
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
      startActivityAndCollapse(PendingIntent.getActivity(this, 0, intent, PendingIntent.FLAG_IMMUTABLE))
    } else {
      startActivityAndCollapse(intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }
  }

  companion object {
    /** The tile while Quick Settings shows it. */
    @Volatile private var listening: ModeTile? = null
    private val main = Handler(Looper.getMainLooper())

    /** Shows the copy on the tile: now if it's showing, else when it's next shown. */
    fun update(context: Context) {
      TileService.requestListeningState(context, ComponentName(context, ModeTile::class.java))
      main.post { listening?.show() }
    }
  }
}

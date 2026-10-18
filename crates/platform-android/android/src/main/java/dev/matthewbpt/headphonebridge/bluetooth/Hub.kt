package dev.matthewbpt.headphonebridge.bluetooth

import android.Manifest
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothManager
import android.content.Context
import android.content.pm.PackageManager
import android.util.Log
import androidx.core.content.ContextCompat
import org.json.JSONObject

internal const val TAG = "HeadphoneBridge"

/** Whether the app has the Nearby devices permission, without starting Bluetooth. */
fun hasNearbyDevicesPermission(context: Context): Boolean =
  ContextCompat.checkSelfPermission(context, Manifest.permission.BLUETOOTH_CONNECT) ==
    PackageManager.PERMISSION_GRANTED

internal fun bluetoothAdapter(context: Context): BluetoothAdapter? =
  (context.getSystemService(Context.BLUETOOTH_SERVICE) as BluetoothManager?)?.adapter

/**
 * Bluetooth for the whole process, shared by the App Screen, the widget and
 * the tile. It starts the core and routes its messages: [DeviceWatch] reports
 * every Connected Device with a vendor service and the media output,
 * [ControlLinks] opens the control links the core asks for, and
 * [NativeCoreLink] keeps the latest Device list. The core decides when to
 * Attach and Detach.
 */
object Hub : CoreLink by NativeCoreLink {
  private var started = false
  private var watching = false

  /** Starts the core, once per process, and watches Bluetooth if allowed. */
  fun ensureStarted(from: Context) {
    synchronized(this) {
      if (!started) {
        val context = from.applicationContext
        NativeCoreLink.context = context
        Log.i(TAG, "hub: starting the core")
        // The frame log is on before any Attach, even when the widget or
        // tile starts the core.
        NativeCore.start(frameLogIfOn(context.dataDir))
        started = true
      }
    }
    startWatchingIfAllowed()
  }

  /** Starts watching once the Nearby devices permission is granted. */
  fun startWatchingIfAllowed() {
    synchronized(this) {
      if (!started || watching) return
      if (!hasPermission()) {
        Log.i(TAG, "hub: no Nearby devices permission yet")
        return
      }
      watching = true
    }
    DeviceWatch.start(NativeCoreLink.context)
  }

  /** A message from the core, on a Rust thread. Must not block. */
  fun fromCore(message: JSONObject) {
    when (message.getString("type")) {
      "open" -> {
        ControlLinks.open(
          bluetoothAdapter(NativeCoreLink.context),
          hasPermission(),
          message.getLong("request"),
          message.getString("address"),
          message.getString("uuid"),
        )
      }

      "write" -> {
        ControlLinks.write(message.getInt("link"), message.getJSONArray("bytes"))
      }

      "close" -> {
        ControlLinks.close(message.getInt("link"), "the core is done with it")
      }

      "deviceList" -> {
        NativeCoreLink.updateDeviceList(message.getJSONObject("list"))
      }

      "commandResult" -> {
        val error = if (message.isNull("error")) null else message.get("error").toString()
        NativeCoreLink.answer(message.getLong("request"), error)
      }
    }
  }
}

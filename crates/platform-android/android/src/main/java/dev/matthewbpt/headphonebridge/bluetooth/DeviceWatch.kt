package dev.matthewbpt.headphonebridge.bluetooth

import android.annotation.SuppressLint
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothProfile
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.media.AudioAttributes
import android.media.AudioDeviceCallback
import android.media.AudioDeviceInfo
import android.media.AudioManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.ParcelUuid
import android.util.Log
import androidx.core.content.ContextCompat
import androidx.core.content.IntentCompat
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.ConcurrentHashMap

/** How long to wait for an SDP answer before reporting a Device without UUIDs. */
private const val SDP_TIMEOUT_MS = 8000L

/**
 * Watches Bluetooth for [Hub], reporting to the core every Connected Device
 * with a vendor service, and the Device media plays through.
 */
@SuppressLint("MissingPermission")
internal object DeviceWatch {
  private lateinit var context: Context
  private val main = Handler(Looper.getMainLooper())

  private val audio: AudioManager
    get() = context.getSystemService(Context.AUDIO_SERVICE) as AudioManager

  /** Devices reported Connected, or waiting on SDP for their UUIDs. */
  private val reported = ConcurrentHashMap.newKeySet<String>()
  private val awaitingUuids = ConcurrentHashMap.newKeySet<String>()
  private var audioOutput: String? = null
  private var audioOutputReported = false

  /** Starts watching, once; the Nearby devices permission must be granted. */
  fun start(from: Context) {
    context = from
    Log.i(TAG, "hub: watching")
    val filter =
      IntentFilter().apply {
        addAction(BluetoothDevice.ACTION_ACL_CONNECTED)
        addAction(BluetoothDevice.ACTION_ACL_DISCONNECTED)
        addAction(BluetoothDevice.ACTION_UUID)
      }
    // The system sends these, so the receiver must be exported.
    ContextCompat.registerReceiver(context, receiver, filter, ContextCompat.RECEIVER_EXPORTED)
    audio.registerAudioDeviceCallback(audioCallback, main)
    reportAlreadyConnected()
    reportAudioOutput()
  }

  /** Reports the Devices that were Connected before the app started. */
  private fun reportAlreadyConnected() {
    val adapter = bluetoothAdapter(context) ?: return
    for (device in adapter.bondedDevices.orEmpty()) {
      // Hidden API; the profile proxies below catch what it misses.
      val connected =
        try {
          device.javaClass.getMethod("isConnected").invoke(device) as? Boolean == true
        } catch (_: ReflectiveOperationException) {
          false
        }
      if (connected) report(device)
    }
    for (profile in listOf(BluetoothProfile.A2DP, BluetoothProfile.HEADSET)) {
      adapter.getProfileProxy(
        context,
        object : BluetoothProfile.ServiceListener {
          override fun onServiceConnected(
            p: Int,
            proxy: BluetoothProfile,
          ) {
            proxy.connectedDevices.forEach { report(it) }
            adapter.closeProfileProxy(p, proxy)
          }

          override fun onServiceDisconnected(p: Int) {
            // The proxy is closed as soon as it connects.
          }
        },
        profile,
      )
    }
  }

  private val receiver =
    object : BroadcastReceiver() {
      override fun onReceive(
        c: Context,
        intent: Intent,
      ) {
        // The typed overload is API 33; IntentCompat falls back below it.
        val device =
          IntentCompat.getParcelableExtra(intent, BluetoothDevice.EXTRA_DEVICE, BluetoothDevice::class.java)
            ?: return
        when (intent.action) {
          BluetoothDevice.ACTION_ACL_CONNECTED -> {
            report(device)
          }

          BluetoothDevice.ACTION_ACL_DISCONNECTED -> {
            awaitingUuids.remove(device.address)
            if (reported.remove(device.address)) {
              sendToCore(coreEvent("disconnected").put("address", device.address))
            }
          }

          BluetoothDevice.ACTION_UUID -> {
            @Suppress("DEPRECATION")
            val uuids =
              intent
                .getParcelableArrayExtra(BluetoothDevice.EXTRA_UUID)
                ?.mapNotNull { (it as? ParcelUuid)?.uuid?.toString() }
            if (awaitingUuids.remove(device.address)) {
              reportConnected(device, uuids ?: uuidsOf(device))
            }
          }
        }
      }
    }

  /** Reports [device] once its service UUIDs are known, asking SDP if need be. */
  private fun report(device: BluetoothDevice) {
    val address = device.address
    if (reported.contains(address) || !awaitingUuids.add(address)) return
    val uuids = uuidsOf(device)
    if (uuids.isNotEmpty()) {
      awaitingUuids.remove(address)
      return reportConnected(device, uuids)
    }
    device.fetchUuidsWithSdp()
    main.postDelayed({
      if (awaitingUuids.remove(address)) reportConnected(device, uuidsOf(device))
    }, SDP_TIMEOUT_MS)
  }

  /** Reports a Connected Device with a vendor service; the rest aren't the core's. */
  private fun reportConnected(
    device: BluetoothDevice,
    uuids: List<String>,
  ) {
    if (uuids.none { it.lowercase() in VENDOR_SERVICES }) return
    if (!reported.add(device.address)) return
    Log.i(TAG, "hub: ${device.address} connected")
    sendToCore(
      coreEvent("connected")
        .put("address", device.address)
        .put("name", device.name ?: device.alias ?: "")
        .put("uuids", JSONArray(uuids)),
    )
  }

  private fun uuidsOf(device: BluetoothDevice): List<String> = device.uuids?.map { it.uuid.toString() }.orEmpty()

  private val audioCallback =
    object : AudioDeviceCallback() {
      override fun onAudioDevicesAdded(added: Array<out AudioDeviceInfo>) = reportAudioOutput()

      override fun onAudioDevicesRemoved(removed: Array<out AudioDeviceInfo>) = reportAudioOutput()
    }

  private val bluetoothOutputTypes =
    setOf(
      AudioDeviceInfo.TYPE_BLUETOOTH_A2DP,
      AudioDeviceInfo.TYPE_BLE_HEADSET,
      AudioDeviceInfo.TYPE_BLE_SPEAKER,
    )

  /** Reports the Bluetooth Device media plays through, by its Classic address, if any. */
  private fun reportAudioOutput() {
    val outputs =
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
        val media = AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).build()
        audio.getAudioDevicesForAttributes(media)
      } else {
        audio.getDevices(AudioManager.GET_DEVICES_OUTPUTS).toList()
      }
    val address = outputs.firstOrNull { it.type in bluetoothOutputTypes }?.address?.ifEmpty { null }
    synchronized(this) {
      if (audioOutputReported && address == audioOutput) return
      audioOutputReported = true
      audioOutput = address
    }
    Log.i(TAG, "hub: media output $address")
    sendToCore(coreEvent("audioOutput").put("address", address ?: JSONObject.NULL))
  }
}

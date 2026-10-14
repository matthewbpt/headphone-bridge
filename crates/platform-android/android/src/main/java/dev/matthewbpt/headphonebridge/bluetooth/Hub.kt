package dev.matthewbpt.headphonebridge.bluetooth

import android.Manifest
import android.annotation.SuppressLint
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothManager
import android.bluetooth.BluetoothProfile
import android.bluetooth.BluetoothSocket
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
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
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong

internal const val TAG = "HeadphoneBridge"

/** How long to wait for an SDP answer before reporting a Device without UUIDs. */
private const val SDP_TIMEOUT_MS = 8000L

/**
 * Bluetooth for the whole process, shared by the app screen, the widget and
 * the tile. It reports every Connected Device with a vendor service and the
 * media output to the core, opens the control links the core asks for, and
 * keeps the latest Device list. The core decides when to Attach and Detach.
 */
@SuppressLint("MissingPermission")
object Hub : CoreLink {
  private lateinit var context: Context
  private var started = false
  private var watching = false
  private val main = Handler(Looper.getMainLooper())

  /** Starts the core, once per process, and watches Bluetooth if allowed. */
  fun ensureStarted(from: Context) {
    synchronized(this) {
      if (!started) {
        context = from.applicationContext
        Log.i(TAG, "hub: starting the core")
        NativeCore.start()
        started = true
      }
    }
    startWatchingIfAllowed()
  }

  /** Whether the app has the Nearby devices permission. Call [ensureStarted] first. */
  override fun hasPermission(): Boolean =
    ContextCompat.checkSelfPermission(context, Manifest.permission.BLUETOOTH_CONNECT) ==
      PackageManager.PERMISSION_GRANTED

  private fun send(event: JSONObject) = NativeCore.event(event.toString())

  private fun event(type: String) = JSONObject().put("type", type)

  // ---- holds ----

  override fun holdAll() = send(event("holdAll"))
  override fun endHoldAll() = send(event("endHoldAll"))
  override fun holdPrimary() = send(event("holdPrimary"))
  override fun endHoldPrimary() = send(event("endHoldPrimary"))

  // ---- watching ----

  private val adapter: BluetoothAdapter?
    get() = (context.getSystemService(Context.BLUETOOTH_SERVICE) as BluetoothManager?)?.adapter
  private val audio: AudioManager
    get() = context.getSystemService(Context.AUDIO_SERVICE) as AudioManager

  /** Devices reported Connected, or waiting on SDP for their UUIDs. */
  private val reported = ConcurrentHashMap.newKeySet<String>()
  private val awaitingUuids = ConcurrentHashMap.newKeySet<String>()
  private var audioOutput: String? = null
  private var audioOutputReported = false

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
    Log.i(TAG, "hub: watching")
    val filter = IntentFilter().apply {
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
    val adapter = adapter ?: return
    for (device in adapter.bondedDevices.orEmpty()) {
      // Hidden API; the profile proxies below catch what it misses.
      val connected = try {
        device.javaClass.getMethod("isConnected").invoke(device) as Boolean
      } catch (e: Exception) {
        false
      }
      if (connected) report(device)
    }
    for (profile in listOf(BluetoothProfile.A2DP, BluetoothProfile.HEADSET)) {
      adapter.getProfileProxy(context, object : BluetoothProfile.ServiceListener {
        override fun onServiceConnected(p: Int, proxy: BluetoothProfile) {
          proxy.connectedDevices.forEach { report(it) }
          adapter.closeProfileProxy(p, proxy)
        }

        override fun onServiceDisconnected(p: Int) {}
      }, profile)
    }
  }

  private val receiver = object : BroadcastReceiver() {
    override fun onReceive(c: Context, intent: Intent) {
      // The typed overload is API 33; IntentCompat falls back below it.
      val device =
        IntentCompat.getParcelableExtra(intent, BluetoothDevice.EXTRA_DEVICE, BluetoothDevice::class.java)
          ?: return
      when (intent.action) {
        BluetoothDevice.ACTION_ACL_CONNECTED -> report(device)
        BluetoothDevice.ACTION_ACL_DISCONNECTED -> {
          awaitingUuids.remove(device.address)
          if (reported.remove(device.address)) {
            send(event("disconnected").put("address", device.address))
          }
        }
        BluetoothDevice.ACTION_UUID -> {
          @Suppress("DEPRECATION")
          val uuids = intent.getParcelableArrayExtra(BluetoothDevice.EXTRA_UUID)
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
  private fun reportConnected(device: BluetoothDevice, uuids: List<String>) {
    if (uuids.none { it.lowercase() in VENDOR_SERVICES }) return
    if (!reported.add(device.address)) return
    Log.i(TAG, "hub: ${device.address} connected")
    send(
      event("connected")
        .put("address", device.address)
        .put("name", device.name ?: device.alias ?: "")
        .put("uuids", JSONArray(uuids))
    )
  }

  private fun uuidsOf(device: BluetoothDevice): List<String> =
    device.uuids?.map { it.uuid.toString() }.orEmpty()

  private val audioCallback = object : AudioDeviceCallback() {
    override fun onAudioDevicesAdded(added: Array<out AudioDeviceInfo>) = reportAudioOutput()
    override fun onAudioDevicesRemoved(removed: Array<out AudioDeviceInfo>) = reportAudioOutput()
  }

  private val bluetoothOutputTypes = setOf(
    AudioDeviceInfo.TYPE_BLUETOOTH_A2DP,
    AudioDeviceInfo.TYPE_BLE_HEADSET,
    AudioDeviceInfo.TYPE_BLE_SPEAKER,
  )

  /** Reports the Bluetooth Device media plays through, by its Classic address, if any. */
  private fun reportAudioOutput() {
    val outputs = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
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
    send(event("audioOutput").put("address", address ?: JSONObject.NULL))
  }

  // ---- messages from the core ----

  /** A message from the core, on a Rust thread. Must not block. */
  fun fromCore(message: JSONObject) {
    when (message.getString("type")) {
      "open" -> open(message.getLong("request"), message.getString("address"), message.getString("uuid"))
      "write" -> write(message.getInt("link"), message.getJSONArray("bytes"))
      "close" -> closeLink(message.getInt("link"), "the core is done with it")
      "deviceList" -> onDeviceList(message.getJSONObject("list"))
      "commandResult" -> {
        val error = if (message.isNull("error")) null else message.get("error").toString()
        pendingCommands.remove(message.getLong("request"))?.invoke(error)
      }
    }
  }

  // ---- control links ----

  /** An open control link: its socket, and the one thread that writes to it, in order. */
  private class Link(val socket: BluetoothSocket, val writer: ExecutorService)

  private val links = ConcurrentHashMap<Int, Link>()
  private val nextLinkId = AtomicInteger(1)

  private fun openFailed(request: Long, permission: Boolean, message: String) = send(
    event("openFailed").put("request", request).put("permission", permission).put("message", message)
  )

  private fun open(request: Long, address: String, uuid: String) {
    if (!hasPermission()) return openFailed(request, true, "no Nearby devices permission")
    Thread({
      try {
        val device = adapter?.getRemoteDevice(address)
          ?: return@Thread openFailed(request, false, "no Bluetooth adapter")
        val service = UUID.fromString(uuid)
        val failures = mutableListOf<String>()
        var socket: BluetoothSocket? = null
        for (kind in socketKinds(uuid)) {
          val started = System.currentTimeMillis()
          try {
            val candidate = when (kind) {
              SocketKind.SECURE -> device.createRfcommSocketToServiceRecord(service)
              SocketKind.INSECURE -> device.createInsecureRfcommSocketToServiceRecord(service)
            }
            try {
              candidate.connect()
            } catch (e: Exception) {
              try { candidate.close() } catch (_: Exception) {}
              throw e
            }
            Log.i(TAG, "link to $address: $kind in ${System.currentTimeMillis() - started} ms")
            socket = candidate
            break
          } catch (e: SecurityException) {
            return@Thread openFailed(request, true, "${e.message}")
          } catch (e: Exception) {
            failures.add("$kind after ${System.currentTimeMillis() - started} ms: ${e.javaClass.simpleName}: ${e.message}")
          }
        }
        val opened = socket ?: return@Thread openFailed(request, false, failures.joinToString("; "))
        val linkId = nextLinkId.getAndIncrement()
        links[linkId] = Link(opened, Executors.newSingleThreadExecutor())
        send(event("opened").put("request", request).put("link", linkId))
        startReader(linkId, opened)
      } catch (e: Exception) {
        // Every open must be answered, or the core waits for ever.
        openFailed(request, false, "${e.javaClass.simpleName}: ${e.message}")
      }
    }, "rfcomm-open-$address").start()
  }

  /** Reads [socket] on its own thread until it closes, then tells the core. */
  private fun startReader(linkId: Int, socket: BluetoothSocket) {
    Thread({
      val buffer = ByteArray(4096)
      try {
        val input = socket.inputStream
        while (true) {
          val n = input.read(buffer)
          if (n < 0) break
          val bytes = JSONArray()
          for (i in 0 until n) bytes.put(buffer[i].toInt() and 0xff)
          send(event("data").put("link", linkId).put("bytes", bytes))
        }
      } catch (e: Exception) {
        Log.i(TAG, "link $linkId: read ended, ${e.message}")
      }
      closeLink(linkId, "read ended")
      send(event("closed").put("link", linkId))
    }, "rfcomm-read-$linkId").start()
  }

  private fun write(linkId: Int, bytes: JSONArray) {
    val link = links[linkId] ?: return
    val data = ByteArray(bytes.length()) { bytes.getInt(it).toByte() }
    try {
      link.writer.execute {
        try {
          link.socket.outputStream.write(data)
          link.socket.outputStream.flush()
        } catch (e: Exception) {
          // Closing the socket ends the reader, which tells the core.
          closeLink(linkId, "write failed, ${e.message}")
        }
      }
    } catch (e: Exception) {
      // The link closed meanwhile.
    }
  }

  private fun closeLink(linkId: Int, why: String) {
    val link = links.remove(linkId) ?: return
    Log.i(TAG, "link $linkId: closing ($why)")
    link.writer.shutdown()
    try {
      link.socket.close()
    } catch (_: Exception) {}
  }

  // ---- the Device list and commands ----

  private val listLock = Object()
  @Volatile override var deviceList: JSONObject? = null
    private set

  private val listListeners = CopyOnWriteArrayList<(JSONObject) -> Unit>()

  private fun onDeviceList(list: JSONObject) {
    synchronized(listLock) {
      deviceList = list
      listLock.notifyAll()
    }
    listListeners.forEach { it(list) }
  }

  override fun onDeviceList(listener: (JSONObject) -> Unit) {
    listListeners.add(listener)
  }

  override fun awaitDeviceList(timeoutMs: Long, ready: (JSONObject) -> Boolean): JSONObject? {
    val deadline = System.currentTimeMillis() + timeoutMs
    synchronized(listLock) {
      while (true) {
        deviceList?.let { if (ready(it)) return it }
        val left = deadline - System.currentTimeMillis()
        if (left <= 0) return null
        listLock.wait(left)
      }
    }
  }

  private val pendingCommands = ConcurrentHashMap<Long, (String?) -> Unit>()
  private val nextRequest = AtomicLong(1)

  override fun command(command: JSONObject, timeoutMs: Long): String? {
    val request = nextRequest.getAndIncrement()
    val done = CountDownLatch(1)
    var error: String? = null
    pendingCommands[request] = { e ->
      error = e
      done.countDown()
    }
    NativeCore.command(request, command.toString())
    if (!done.await(timeoutMs, TimeUnit.MILLISECONDS)) {
      pendingCommands.remove(request)
      return "no answer"
    }
    return error
  }
}

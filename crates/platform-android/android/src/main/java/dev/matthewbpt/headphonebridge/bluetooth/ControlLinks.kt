package dev.matthewbpt.headphonebridge.bluetooth

import android.annotation.SuppressLint
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothSocket
import android.util.Log
import org.json.JSONArray
import java.io.IOException
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.atomic.AtomicInteger

/** The most a control link's reader takes from its socket at once. */
private const val READ_BUFFER_BYTES = 4096

// Copies of LinkFailure's serde names, in crates/core/src/platform.rs, for
// why a control link couldn't be opened. Keep them in step.
private const val PERMISSION_DENIED = "bluetoothPermissionDenied"
private const val OTHER = "other"

/** The RFCOMM control links the core asks [Hub] to open, by link id. */
@SuppressLint("MissingPermission")
internal object ControlLinks {
  /** An open control link: its socket, and the one thread that writes to it, in order. */
  private class Link(
    val socket: BluetoothSocket,
    val writer: ExecutorService,
  )

  private val links = ConcurrentHashMap<Int, Link>()
  private val nextLinkId = AtomicInteger(1)

  /**
   * Opens a control link to [uuid] on the Device at [address], on its own
   * thread, and answers the core's [request] with `opened` or `openFailed`.
   */
  fun open(
    adapter: BluetoothAdapter?,
    permitted: Boolean,
    request: Long,
    address: String,
    uuid: String,
  ) {
    if (!permitted) return openFailed(request, PERMISSION_DENIED)
    Thread({ answerOpen(adapter, request, address, uuid) }, "rfcomm-open-$address").start()
  }

  // Every open must be answered, or the core waits for ever, so whatever
  // goes wrong becomes an `openFailed`.
  @Suppress("TooGenericExceptionCaught")
  private fun answerOpen(
    adapter: BluetoothAdapter?,
    request: Long,
    address: String,
    uuid: String,
  ) {
    try {
      val device = adapter?.getRemoteDevice(address) ?: throw IOException("no Bluetooth adapter")
      val socket = connect(device, uuid)
      val linkId = nextLinkId.getAndIncrement()
      links[linkId] = Link(socket, Executors.newSingleThreadExecutor())
      sendToCore(coreEvent("opened").put("request", request).put("link", linkId))
      startReader(linkId, socket)
    } catch (e: SecurityException) {
      Log.i(TAG, "open to $address: ${e.message}")
      openFailed(request, PERMISSION_DENIED)
    } catch (e: IOException) {
      openFailedOther(request, e.message ?: e.javaClass.simpleName)
    } catch (e: Exception) {
      openFailedOther(request, "${e.javaClass.simpleName}: ${e.message}")
    }
  }

  /**
   * A socket connected to [device]'s [uuid] service, trying each socket kind
   * in turn. Throws an [IOException] naming every kind's failure.
   */
  private fun connect(
    device: BluetoothDevice,
    uuid: String,
  ): BluetoothSocket {
    val service = UUID.fromString(uuid)
    val failures = mutableListOf<String>()
    for (kind in socketKinds(uuid)) {
      val started = System.currentTimeMillis()
      try {
        val socket =
          when (kind) {
            SocketKind.SECURE -> device.createRfcommSocketToServiceRecord(service)
            SocketKind.INSECURE -> device.createInsecureRfcommSocketToServiceRecord(service)
          }
        connectOrClose(socket)
        Log.i(TAG, "link to ${device.address}: $kind in ${System.currentTimeMillis() - started} ms")
        return socket
      } catch (e: IOException) {
        val elapsed = System.currentTimeMillis() - started
        failures.add("$kind after $elapsed ms: ${e.javaClass.simpleName}: ${e.message}")
      }
    }
    throw IOException(failures.joinToString("; "))
  }

  private fun connectOrClose(socket: BluetoothSocket) {
    try {
      socket.connect()
    } catch (e: IOException) {
      closeQuietly(socket)
      throw e
    } catch (e: SecurityException) {
      closeQuietly(socket)
      throw e
    }
  }

  /** Answers [request] with the link failure named [reason], a cause with no detail. */
  private fun openFailed(
    request: Long,
    reason: String,
  ) = sendToCore(coreEvent("openFailed").put("request", request).put("reason", reason))

  /** Answers [request] with a link failure that has no cause, only its [detail]. */
  private fun openFailedOther(
    request: Long,
    detail: String,
  ) = sendToCore(coreEvent("openFailed").put("request", request).put("reason", OTHER).put("detail", detail))

  /** Reads [socket] on its own thread until it closes, then tells the core. */
  private fun startReader(
    linkId: Int,
    socket: BluetoothSocket,
  ) {
    Thread({
      val buffer = ByteArray(READ_BUFFER_BYTES)
      try {
        val input = socket.inputStream
        while (true) {
          val n = input.read(buffer)
          if (n < 0) break
          val bytes = JSONArray()
          for (i in 0 until n) bytes.put(java.lang.Byte.toUnsignedInt(buffer[i]))
          sendToCore(coreEvent("data").put("link", linkId).put("bytes", bytes))
        }
      } catch (e: IOException) {
        Log.i(TAG, "link $linkId: read ended, ${e.message}")
      } finally {
        // However reading ends, the core hears the link dropped.
        close(linkId, "read ended")
        sendToCore(coreEvent("closed").put("link", linkId))
      }
    }, "rfcomm-read-$linkId").start()
  }

  fun write(
    linkId: Int,
    bytes: JSONArray,
  ) {
    val link = links[linkId] ?: return
    val data = ByteArray(bytes.length()) { bytes.getInt(it).toByte() }
    try {
      link.writer.execute {
        try {
          link.socket.outputStream.write(data)
          link.socket.outputStream.flush()
        } catch (e: IOException) {
          // Closing the socket ends the reader, which tells the core.
          close(linkId, "write failed, ${e.message}")
        }
      }
    } catch (_: RejectedExecutionException) {
      // The link closed meanwhile.
    }
  }

  fun close(
    linkId: Int,
    why: String,
  ) {
    val link = links.remove(linkId) ?: return
    Log.i(TAG, "link $linkId: closing ($why)")
    link.writer.shutdown()
    closeQuietly(link.socket)
  }

  private fun closeQuietly(socket: BluetoothSocket) {
    try {
      socket.close()
    } catch (_: IOException) {
      // Closing is best effort: the socket is unusable either way.
    }
  }
}

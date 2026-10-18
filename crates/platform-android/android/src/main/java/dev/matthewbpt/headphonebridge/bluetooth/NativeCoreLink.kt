package dev.matthewbpt.headphonebridge.bluetooth

import android.content.Context
import org.json.JSONObject
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicLong

/**
 * The real [CoreLink], over [NativeCore]: holds, commands and the latest
 * Device list. [Hub] sets [context] when it starts the core.
 */
internal object NativeCoreLink : CoreLink {
  lateinit var context: Context

  /** Whether the app has the Nearby devices permission. Call [Hub.ensureStarted] first. */
  override fun hasPermission(): Boolean = hasNearbyDevicesPermission(context)

  // ---- holds ----

  override fun holdAll() = sendToCore(coreEvent("holdAll"))

  override fun endHoldAll() = sendToCore(coreEvent("endHoldAll"))

  override fun holdPrimary() = sendToCore(coreEvent("holdPrimary"))

  override fun endHoldPrimary() = sendToCore(coreEvent("endHoldPrimary"))

  // ---- the Device list ----

  private val listLock = Object()

  @Volatile override var deviceList: JSONObject? = null
    private set

  private val listListeners = CopyOnWriteArrayList<(JSONObject) -> Unit>()

  /** Takes a Device list from the core. */
  fun updateDeviceList(list: JSONObject) {
    synchronized(listLock) {
      deviceList = list
      listLock.notifyAll()
    }
    listListeners.forEach { it(list) }
  }

  override fun onDeviceList(listener: (JSONObject) -> Unit) {
    listListeners.add(listener)
  }

  override fun awaitDeviceList(
    timeoutMs: Long,
    ready: (JSONObject) -> Boolean,
  ): JSONObject? {
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

  // ---- commands ----

  private val pendingCommands = ConcurrentHashMap<Long, (String?) -> Unit>()
  private val nextRequest = AtomicLong(1)

  override fun command(
    command: JSONObject,
    timeoutMs: Long,
  ): String? {
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

  /** Takes the core's answer to command [request]: null if accepted, else why not. */
  fun answer(
    request: Long,
    error: String?,
  ) {
    pendingCommands.remove(request)?.invoke(error)
  }
}

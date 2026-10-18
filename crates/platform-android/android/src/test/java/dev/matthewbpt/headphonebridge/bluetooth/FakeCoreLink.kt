package dev.matthewbpt.headphonebridge.bluetooth

import org.json.JSONArray
import org.json.JSONObject

/**
 * A core with one Connected Device, the Primary Device, that answers holds
 * and commands the way the real one does: holding Attaches it, a set is
 * confirmed by a new Device list, and ending the hold Detaches it with its
 * last reported values.
 */
class FakeCoreLink(
  val address: String = "AA:BB",
  var name: String = "Galaxy Buds4 Pro",
  var modes: List<String> = listOf("off", "noiseCancelling", "ambient", "adaptive"),
  var chosen: String = "off",
) : CoreLink {
  /** Everything sent to the core, in order, e.g. `holdPrimary` or `set ambient`. */
  val sent = mutableListOf<String>()

  /** Whether holding the Device Attaches it. */
  var attaches = true

  /** Whether the Device confirms a set. */
  var confirms = true

  /** What the core answers a command with; null accepts it. */
  var commandError: String? = null

  /** Whether the app has the Nearby devices permission. */
  var permitted = true

  /** The state the Device is in until it's held, as the core reports it. */
  var idleState: JSONObject = state("detached")

  private val lock = Object()
  private val listeners = mutableListOf<(JSONObject) -> Unit>()
  private var held = false

  /** The shell's labels, with a reason table of the test's own words. */
  override var labels: JSONObject = labels()

  @Volatile override var deviceList: JSONObject? = null
    private set

  /** Reports the Device Connected and not yet Attached. */
  fun connect() = publishDevice(device(idleState))

  /** Reports no Device Connected. */
  fun disconnect() = publishDevice(null)

  /** Attaches the held Device, e.g. once another app lets go of it. */
  fun attach() = publishDevice(device(state("attached")))

  /** Sends [list] as the core's latest Device list. */
  fun publish(list: JSONObject) {
    // In order, as the core sends them from one thread.
    synchronized(lock) {
      deviceList = list
      lock.notifyAll()
      listeners.forEach { it(list) }
    }
  }

  private fun publishDevice(device: JSONObject?) =
    publish(
      JSONObject()
        .put("devices", JSONArray().apply { device?.let { put(it) } })
        .put("primary", device?.getString("address") ?: JSONObject.NULL),
    )

  private fun device(state: JSONObject) = device(address, name, state, modes, chosen)

  private fun record(what: String) {
    synchronized(sent) { sent.add(what) }
  }

  override fun hasPermission() = permitted

  override fun holdAll() = record("holdAll")

  override fun endHoldAll() = record("endHoldAll")

  override fun holdPrimary() {
    record("holdPrimary")
    if (deviceList?.isNull("primary") != false) return
    held = true
    if (attaches) attach()
  }

  override fun endHoldPrimary() {
    record("endHoldPrimary")
    if (!held) return
    held = false
    publishDevice(device(state("detached")).put("reportedAt", 1_000L))
  }

  override fun command(
    command: JSONObject,
    timeoutMs: Long,
  ): String? {
    val mode = command.getString("mode")
    record("set $mode")
    if (commandError != null) return commandError
    if (confirms) {
      chosen = mode
      attach()
    }
    return null
  }

  override fun awaitDeviceList(
    timeoutMs: Long,
    ready: (JSONObject) -> Boolean,
  ): JSONObject? {
    val deadline = System.currentTimeMillis() + timeoutMs
    synchronized(lock) {
      while (true) {
        deviceList?.let { if (ready(it)) return it }
        val left = deadline - System.currentTimeMillis()
        if (left <= 0) return null
        lock.wait(left)
      }
    }
  }

  override fun onDeviceList(listener: (JSONObject) -> Unit) {
    listeners.add(listener)
  }
}

fun state(kind: String): JSONObject = JSONObject().put("kind", kind)

fun notAttached(reason: String): JSONObject = state("notAttached").put("reason", JSONObject().put("kind", reason))

/**
 * A Device as the core's Device list has it. Tests name only the parameters
 * they care about; the rest default.
 */
@Suppress("LongParameterList")
fun device(
  address: String,
  name: String,
  state: JSONObject,
  modes: List<String> = emptyList(),
  chosen: String? = null,
  battery: JSONObject? = null,
): JSONObject =
  JSONObject()
    .put("address", address)
    .put("name", name)
    .put("state", state)
    .put("capabilities", JSONObject().put("noiseControlModes", JSONArray(modes)))
    .put("chosenMode", chosen ?: JSONObject.NULL)
    .put("battery", battery ?: JSONObject.NULL)
    .put("reportedAt", JSONObject.NULL)

/** A Device list with [devices], the first being the Primary Device. */
fun deviceList(vararg devices: JSONObject): JSONObject =
  JSONObject()
    .put("devices", JSONArray(devices.toList()))
    .put("primary", devices.firstOrNull()?.getString("address") ?: JSONObject.NULL)

/** Labels as the shell's `labels::all` serialises them, with a reason table of [reasons]. */
fun labels(vararg reasons: Pair<String, String>): JSONObject =
  JSONObject().put(
    "reasons",
    JSONArray(
      reasons.map { (kind, text) ->
        JSONObject().put("reason", JSONObject().put("kind", kind)).put("text", text)
      },
    ),
  )

package dev.matthewbpt.headphonebridge.bluetooth

import org.json.JSONArray
import org.json.JSONObject

/**
 * What the widget and tile show, shared by both and kept across process
 * deaths: the Primary Device, what each Device last reported, by address, and
 * what an action is doing.
 */
class StoredCopy(
  private val storage: Storage,
  private val now: () -> Long,
  /** Called after every change, for the surfaces to show it. */
  private val onChange: () -> Unit = {},
) {
  /** Where the copy is kept: SharedPreferences on the phone. */
  interface Storage {
    fun read(): String?
    fun write(copy: String)
  }

  /** What a surface shows. */
  data class Shown(
    val name: String?,
    val battery: String?,
    /** The Primary Device's Noise Control Modes, in Capability order; empty if not known. */
    val modes: List<String>,
    /** The mode to highlight. */
    val mode: String?,
    /** When the mode and battery were last reported, in ms since the epoch, if ever. */
    val updatedAt: Long?,
    /** What an action is doing, why it failed, or why there's nothing to do; null to show [updatedAt]. */
    val note: String?,
    /** Whether a tap anywhere opens the app, as there's nothing else it can do. */
    val opensApp: Boolean = false,
  )

  /** What a Device last reported. */
  private data class Remembered(
    val name: String,
    val modes: List<String>,
    val chosen: String?,
    val battery: String?,
    val updatedAt: Long?,
  )

  private var primary: String? = null
  private val devices = mutableMapOf<String, Remembered>()
  private var note: String? = null

  /** Whether an action is running, so [note] says what it's doing. */
  private var busy = false

  /** The mode an action is setting, shown as if it already applied. */
  private var pending: String? = null

  init {
    storage.read()?.let { load(JSONObject(it)) }
  }

  @Synchronized
  fun onDeviceList(list: JSONObject) {
    val device = primaryDevice(list)
    primary = device?.getString("address")
    if (device != null) {
      val address = device.getString("address")
      val before = devices[address]
      val modes = device.getJSONObject("capabilities").getJSONArray("noiseControlModes").strings()
      val reported = !device.isNull("chosenMode")
      devices[address] = Remembered(
        name = device.getString("name"),
        modes = modes.ifEmpty { before?.modes.orEmpty() },
        chosen = if (reported) device.getString("chosenMode") else before?.chosen,
        battery = batteryText(device) ?: before?.battery,
        updatedAt = if (reported) device.optLongOrNull("reportedAt") ?: now() else before?.updatedAt,
      )
      // Attached again, e.g. from the App Screen: whatever failed is over.
      if (!busy && device.getJSONObject("state").getString("kind") == "attached") note = null
    }
    save()
    onChange()
  }

  /** An action started, or a tap changed its [target]: null to refresh. */
  @Synchronized
  fun actionStarted(target: String?) {
    busy = true
    pending = target
    note = if (target != null) "Setting ${modeLabel(target)}…" else "Refreshing…"
    onChange()
  }

  @Synchronized
  fun actionFinished(failure: String?) {
    busy = false
    pending = null
    note = failure
    save()
    onChange()
  }

  @Synchronized
  fun shown(hasPermission: Boolean): Shown {
    val device = primary?.let { devices[it] }
    if (!hasPermission) {
      return Shown(device?.name, device?.battery, emptyList(), null, device?.updatedAt, ALLOW_NEARBY_DEVICES, opensApp = true)
    }
    return Shown(
      name = device?.name,
      battery = device?.battery,
      modes = device?.modes.orEmpty(),
      mode = pending ?: device?.chosen,
      updatedAt = device?.updatedAt,
      note = note ?: when {
        device == null -> NO_DEVICE_CONNECTED
        device.modes.isEmpty() -> TAP_TO_REFRESH
        else -> null
      },
    )
  }

  private fun save() {
    val saved = JSONObject()
      .put("primary", primary ?: JSONObject.NULL)
      // An action doesn't outlive the process, but why one failed does.
      .put("note", (if (busy) null else note) ?: JSONObject.NULL)
    val all = JSONObject()
    for ((address, device) in devices) {
      all.put(
        address,
        JSONObject()
          .put("name", device.name)
          .put("modes", JSONArray(device.modes))
          .put("chosen", device.chosen ?: JSONObject.NULL)
          .put("battery", device.battery ?: JSONObject.NULL)
          .put("updatedAt", device.updatedAt ?: JSONObject.NULL),
      )
    }
    storage.write(saved.put("devices", all).toString())
  }

  private fun load(saved: JSONObject) {
    primary = saved.optStringOrNull("primary")
    note = saved.optStringOrNull("note")
    val all = saved.getJSONObject("devices")
    for (address in all.keys()) {
      val device = all.getJSONObject(address)
      devices[address] = Remembered(
        name = device.getString("name"),
        modes = device.getJSONArray("modes").strings(),
        chosen = device.optStringOrNull("chosen"),
        battery = device.optStringOrNull("battery"),
        updatedAt = device.optLongOrNull("updatedAt"),
      )
    }
  }
}

/** The Primary Device in a Device list, or null if there's none. */
internal fun primaryDevice(list: JSONObject): JSONObject? {
  if (list.isNull("primary")) return null
  val primary = list.getString("primary")
  val devices = list.getJSONArray("devices")
  return (0 until devices.length()).map { devices.getJSONObject(it) }
    .firstOrNull { it.getString("address") == primary }
}

private fun JSONArray.strings(): List<String> = (0 until length()).map { getString(it) }

private fun JSONObject.optStringOrNull(key: String): String? = if (isNull(key)) null else getString(key)

private fun JSONObject.optLongOrNull(key: String): Long? = if (isNull(key)) null else getLong(key)

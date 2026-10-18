package dev.matthewbpt.headphonebridge.bluetooth

import androidx.annotation.DrawableRes
import org.json.JSONArray
import org.json.JSONObject

// The widget's and tile's copies of the shell's mode labels, in
// app/src-tauri/src/labels.rs, of the App Screen's mode icons, in
// app/src/deviceArt.tsx, and of its mode order, in app/src/App.tsx. Keep
// them in step, and the widget picker's preview labels in
// res/values/strings.xml.

/**
 * The App Screen's mode order, from hearing everything to hearing nothing,
 * after MODE_ORDER in app/src/App.tsx. The core's Capability order is its own.
 */
val MODE_ORDER = listOf("off", "ambient", "adaptive", "noiseCancelling")

fun modeLabel(mode: String): String =
  when (mode) {
    "off" -> "Off"
    "noiseCancelling" -> "Noise Cancelling"
    "ambient" -> "Ambient"
    "adaptive" -> "Adaptive"
    else -> mode
  }

/** A mode's icon; null for a mode the app doesn't know. */
@DrawableRes
fun modeIcon(mode: String): Int? =
  when (mode) {
    "off" -> R.drawable.ic_mode_off
    "noiseCancelling" -> R.drawable.ic_mode_noise_cancelling
    "ambient" -> R.drawable.ic_mode_ambient
    "adaptive" -> R.drawable.ic_mode_adaptive
    else -> null
  }

/**
 * Why an action couldn't Attach the Primary Device in time, from the latest
 * Device list, in the words of the shell's [labels], as `NativeCore.labels`
 * hands them across.
 */
fun notAttachedText(
  list: JSONObject?,
  labels: JSONObject,
): String {
  val state = list?.let { primaryDevice(it) }?.getJSONObject("state")
  return when {
    state == null -> NO_DEVICE_CONNECTED
    state.getString("kind") != "notAttached" -> reasonText(JSONObject().put("kind", "linkDropped"), labels)
    else -> reasonText(state.getJSONObject("reason"), labels)
  }
}

/**
 * Why the Device is Not Attached, labelled as the shell labels it, with the
 * detail of a link failure without a cause beneath. A reason the shell has
 * no label for is shown by its name, as the window shows it.
 */
private fun reasonText(
  reason: JSONObject,
  labels: JSONObject,
): String {
  val kind = reason.getString("kind")
  val table = labels.optJSONArray("reasons") ?: JSONArray()
  val label =
    (0 until table.length())
      .map { table.getJSONObject(it) }
      .firstOrNull { it.getJSONObject("reason").getString("kind") == kind }
      ?.getString("text") ?: kind
  return listOf(label, reason.optString("detail")).filter { it.isNotEmpty() }.joinToString("\n")
}

const val NO_DEVICE_CONNECTED = "No Device Connected"
const val ALLOW_NEARBY_DEVICES = "Open the app to allow Nearby devices"
const val TAP_TO_REFRESH = "Tap to refresh"

/** A Device's battery, e.g. "L 80% · R 75% · Case 50%" or "57%", or null if it hasn't said. */
fun batteryText(device: JSONObject): String? {
  if (device.isNull("battery")) return null
  val battery = device.getJSONObject("battery")

  fun percent(key: String) = if (battery.isNull(key)) null else "${battery.getInt(key)}%"
  return when (battery.getString("kind")) {
    "headphones" -> {
      percent("level")
    }

    "buds" -> {
      listOfNotNull(
        percent("left")?.let { "L $it" },
        percent("right")?.let { "R $it" },
        percent("case")?.let { "Case $it" },
      ).joinToString(" · ").ifEmpty { null }
    }

    else -> {
      null
    }
  }
}

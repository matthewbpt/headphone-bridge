package dev.matthewbpt.headphonebridge.bluetooth

import org.json.JSONObject

/**
 * Runs the widget's and tile's actions. One per process, shared by both, as
 * the core keeps one hold of the Primary Device at a time and the first end
 * would end it for both.
 *
 * An action holds the Primary Device, waits for it to be Attached, sets the
 * mode, waits for the Device to confirm it, and ends the hold, so the vendor
 * apps can have the control link again. Taps during an action change its
 * target instead of starting another.
 */
class ActionController(
  private val link: CoreLink,
  private val copy: StoredCopy,
  /** Starts Bluetooth; false without the Nearby devices permission. */
  private val start: () -> Boolean,
  private val timeouts: Timeouts = Timeouts(),
  private val run: (() -> Unit) -> Unit = { Thread(it, "headphone-bridge-action").start() },
  private val log: (String) -> Unit = {},
) {
  data class Timeouts(
    /** How long the Primary Device gets to be Attached before the action gives up. */
    val attachMs: Long = 8000,
    val commandMs: Long = 3000,
    val confirmMs: Long = 3000,
  )

  private val lock = Any()
  private var running = false

  /** The mode to set, or null when there's none left to set. */
  private var target: String? = null
  private val whenDone = mutableListOf<() -> Unit>()

  init {
    link.onDeviceList { copy.onDeviceList(it) }
  }

  /**
   * Sets [mode] on the Primary Device, or just refreshes what's shown when
   * [mode] is null. [onDone] runs once the action that takes this tap is over.
   */
  fun tap(mode: String?, onDone: () -> Unit = {}) {
    synchronized(lock) {
      if (mode != null) target = mode
      whenDone.add(onDone)
      copy.actionStarted(target)
      if (running) return
      running = true
    }
    run { work() }
  }

  private fun work() {
    while (true) {
      val failure = round()
      val done = synchronized(lock) {
        // A tap since the last check takes another round, now that the hold
        // has ended, so this round's end doesn't end the next one's hold.
        if (failure == null && target != null) {
          null
        } else {
          running = false
          target = null
          copy.actionFinished(failure)
          whenDone.toList().also { whenDone.clear() }
        }
      }
      if (done != null) {
        done.forEach { it() }
        return
      }
    }
  }

  /** Holds the Primary Device until no target is left. Returns why it failed, or null. */
  private fun round(): String? {
    if (!start()) return ALLOW_NEARBY_DEVICES
    val deadline = System.currentTimeMillis() + timeouts.attachMs
    // On a cold start the core hears of the Connected Devices just after
    // starting, and holding the Primary Device before it has one holds nothing.
    link.awaitDeviceList(timeouts.attachMs) { primaryDevice(it) != null }
      ?: return NO_DEVICE_CONNECTED
    link.holdPrimary()
    try {
      link.awaitDeviceList(deadline - System.currentTimeMillis()) { attachedWithMode(it) }
        ?: return notAttachedText(link.deviceList)
      while (true) {
        val mode = synchronized(lock) { target } ?: return null
        val device = link.deviceList?.let { primaryDevice(it) } ?: return NO_DEVICE_CONNECTED
        if (device.optString("chosenMode") != mode) {
          val error = link.command(
            JSONObject()
              .put("kind", "setNoiseControlMode")
              .put("address", device.getString("address"))
              .put("mode", mode),
            timeouts.commandMs,
          )
          if (error != null) {
            log("action: setting $mode failed, $error")
            return "Couldn't set ${modeLabel(mode)}"
          }
          link.awaitDeviceList(timeouts.confirmMs) { primaryDevice(it)?.optString("chosenMode") == mode }
            ?: return "The Device didn't confirm ${modeLabel(mode)}"
        }
        synchronized(lock) {
          if (target == mode) target = null
        }
      }
    } finally {
      link.endHoldPrimary()
    }
  }

  private fun attachedWithMode(list: JSONObject): Boolean {
    val device = primaryDevice(list) ?: return false
    return device.getJSONObject("state").getString("kind") == "attached" && !device.isNull("chosenMode")
  }
}

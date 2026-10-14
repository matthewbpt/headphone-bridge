package dev.matthewbpt.headphonebridge.bluetooth

import org.json.JSONObject

/**
 * The core's side of the JSON boundary, as the app screen, widget and tile
 * use it. [Hub] is the real one; tests stand in a fake.
 */
interface CoreLink {
  /** Asks the core to hold every Connected Device until [endHoldAll]. */
  fun holdAll()
  fun endHoldAll()

  /** Asks the core to hold the Primary Device until [endHoldPrimary]. */
  fun holdPrimary()
  fun endHoldPrimary()

  /**
   * Runs a command, e.g. `{"kind":"setNoiseControlMode","address":…,"mode":"ambient"}`.
   * Returns null once the core accepts it, else why not: the core's error as
   * JSON text, or "no answer" after [timeoutMs].
   */
  fun command(command: JSONObject, timeoutMs: Long): String?

  /** The latest Device list, or null before the first. */
  val deviceList: JSONObject?

  /** Waits for a Device list [ready] accepts, or returns null after [timeoutMs]. */
  fun awaitDeviceList(timeoutMs: Long, ready: (JSONObject) -> Boolean): JSONObject?
}

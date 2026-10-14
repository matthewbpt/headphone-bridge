package dev.matthewbpt.headphonebridge.bluetooth

import org.json.JSONObject

/**
 * The Rust core, loaded into this process by whichever side asks first: the
 * Tauri app, or Kotlin through [start]. The messages both ways are JSON; see
 * `crates/platform-android/src/messages.rs`.
 */
object NativeCore {
  init {
    System.loadLibrary("headphone_bridge_lib")
  }

  /** Connects Kotlin to the core, starting it if need be. */
  @JvmStatic external fun start()

  /** Something [Hub] observed or finished, or a hold or its end. */
  @JvmStatic external fun event(json: String)

  /** Runs a command; its result comes back as a `commandResult` message. */
  @JvmStatic external fun command(request: Long, json: String)

  /** Every message from the core, on a Rust thread. Must not block. */
  @JvmStatic
  fun fromCore(json: String) {
    Hub.fromCore(JSONObject(json))
  }
}

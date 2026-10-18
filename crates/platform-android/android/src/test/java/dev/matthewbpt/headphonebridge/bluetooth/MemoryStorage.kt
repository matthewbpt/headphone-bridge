package dev.matthewbpt.headphonebridge.bluetooth

/** Storage that lives as long as the test, standing in for SharedPreferences. */
class MemoryStorage : StoredCopy.Storage {
  private var saved: String? = null

  override fun read(): String? = saved

  override fun write(copy: String) {
    saved = copy
  }
}

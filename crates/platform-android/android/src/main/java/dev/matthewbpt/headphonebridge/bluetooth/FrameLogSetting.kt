package dev.matthewbpt.headphonebridge.bluetooth

import java.io.File

/**
 * The frame log file if the frame log setting is on, as the shell keeps it
 * in Tauri's app directories on Android, which are both [dataDir]: the
 * setting is the file `frame-log-on`, and the log is in `logs`. Creates the
 * log's directory. Keep in step with `frame_log_setting` and
 * `frame_log_path` in `app/src-tauri/src/lib.rs`.
 */
fun frameLogIfOn(dataDir: File): String? {
  if (!File(dataDir, "frame-log-on").exists()) return null
  val logs = File(dataDir, "logs").apply { mkdirs() }
  return File(logs, "frame-log.frames").path
}

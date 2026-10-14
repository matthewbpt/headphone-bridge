package dev.matthewbpt.headphonebridge.bluetooth

import java.io.File

/** The cache subdirectory [SharedFileProvider] serves, as `res/xml/shared_files.xml` names it. */
const val SHARED_DIR = "shared"

/**
 * Copies [file] into [cacheDir]'s [SHARED_DIR], replacing any earlier copy, and
 * returns the copy. A FileProvider can't serve Tauri's app directories, and
 * the copy keeps still while the core goes on appending to the original.
 */
fun copyForSharing(file: File, cacheDir: File): File =
  file.copyTo(File(File(cacheDir, SHARED_DIR), file.name), overwrite = true)

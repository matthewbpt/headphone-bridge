package dev.matthewbpt.headphonebridge.bluetooth

import androidx.core.content.FileProvider

/**
 * Serves the files the app shares, such as the frame log, to the app the
 * listener picks in the share sheet. A class of its own, so its manifest entry
 * doesn't clash with the app's own `FileProvider`.
 */
class SharedFileProvider : FileProvider(R.xml.shared_files)

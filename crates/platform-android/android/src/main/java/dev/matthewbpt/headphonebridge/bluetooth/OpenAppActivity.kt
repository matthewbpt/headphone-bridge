package dev.matthewbpt.headphonebridge.bluetooth

import android.app.Activity
import android.os.Bundle

/**
 * Opens the app when the "Noise Control" tile is long-pressed. Quick
 * Settings opens the activity that handles the tile's preferences, and the
 * app's own activity is the shell's, so this one hands over to it.
 */
class OpenAppActivity : Activity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    packageManager.getLaunchIntentForPackage(packageName)?.let { startActivity(it) }
    finish()
  }
}

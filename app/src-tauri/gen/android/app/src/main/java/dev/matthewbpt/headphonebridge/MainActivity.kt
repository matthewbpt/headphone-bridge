package dev.matthewbpt.headphonebridge

import android.content.res.Configuration
import android.os.Bundle
import android.view.View
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    // Edge to edge, the App Screen would draw under the status and
    // navigation bars, and not every WebView reports them as safe-area
    // insets. So the content stays inside them, and the window background,
    // the page's own colour, shows behind them.
    val content = findViewById<View>(android.R.id.content)
    ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
      val bars = insets.getInsets(
        WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout(),
      )
      view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
      WindowInsetsCompat.CONSUMED
    }
  }

  // The activity handles uiMode itself, so it isn't recreated when the OS
  // switches between light and dark: switch the window background and the
  // bars' icons with it.
  override fun onConfigurationChanged(newConfig: Configuration) {
    super.onConfigurationChanged(newConfig)
    window.setBackgroundDrawableResource(R.color.app_background)
    enableEdgeToEdge()
  }
}

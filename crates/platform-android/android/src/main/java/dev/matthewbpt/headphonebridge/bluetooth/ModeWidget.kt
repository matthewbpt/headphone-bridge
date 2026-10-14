package dev.matthewbpt.headphonebridge.bluetooth

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.text.format.DateUtils
import android.util.Log
import android.view.View
import android.widget.RemoteViews
import androidx.core.content.ContextCompat

/**
 * The "Noise Control" home screen widget. It follows the Primary Device:
 * its name and battery, a button per Noise Control Mode, and what the last
 * action did. Tapping a mode sets it, tapping the name refreshes, and "Open"
 * opens the app. Without the Nearby devices permission, every tap opens the
 * app.
 */
class ModeWidget : AppWidgetProvider() {
  override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) {
    Surfaces.watch(context)
    render(context, manager, ids)
  }

  override fun onReceive(context: Context, intent: Intent) {
    when (intent.action) {
      ACTION_SET_MODE, ACTION_REFRESH -> {
        val mode = intent.getStringExtra(EXTRA_MODE)
        Log.i(TAG, "widget: ${intent.action} $mode")
        // Keeps the process alive until the action is over.
        val result = goAsync()
        Surfaces.controller(context).tap(mode) { result.finish() }
      }
      else -> super.onReceive(context, intent)
    }
  }

  companion object {
    private const val ACTION_SET_MODE = "dev.matthewbpt.headphonebridge.SET_MODE"
    private const val ACTION_REFRESH = "dev.matthewbpt.headphonebridge.REFRESH"
    private const val EXTRA_MODE = "mode"
    private const val REFRESH_REQUEST = 100
    private const val OPEN_REQUEST = 200
    private val MODE_VIEWS = intArrayOf(R.id.mode0, R.id.mode1, R.id.mode2, R.id.mode3)

    fun updateAll(context: Context) {
      val manager = AppWidgetManager.getInstance(context)
      val ids = manager.getAppWidgetIds(ComponentName(context, ModeWidget::class.java))
      if (ids.isNotEmpty()) render(context, manager, ids)
    }

    private fun render(context: Context, manager: AppWidgetManager, ids: IntArray) {
      val shown = Surfaces.copy(context).shown(Surfaces.hasPermission(context))
      val views = RemoteViews(context.packageName, R.layout.mode_widget)
      val open = openApp(context)

      views.setTextViewText(R.id.name, shown.name ?: context.getString(R.string.hb_no_device))
      views.setTextViewText(R.id.battery, shown.battery ?: "")
      views.setOnClickPendingIntent(
        R.id.header,
        if (shown.opensApp) open else broadcast(context, ACTION_REFRESH, null, REFRESH_REQUEST),
      )
      views.setOnClickPendingIntent(R.id.root, if (shown.opensApp) open else null)

      val onAccent = ContextCompat.getColor(context, R.color.hb_on_accent)
      val text = ContextCompat.getColor(context, R.color.hb_text)
      MODE_VIEWS.forEachIndexed { i, id ->
        val mode = shown.modes.getOrNull(i)
        if (mode == null) {
          views.setViewVisibility(id, View.GONE)
          return@forEachIndexed
        }
        val selected = mode == shown.mode
        views.setViewVisibility(id, View.VISIBLE)
        views.setTextViewText(id, modeLabel(mode))
        views.setInt(
          id,
          "setBackgroundResource",
          if (selected) R.drawable.mode_selected else R.drawable.mode_unselected,
        )
        views.setTextColor(id, if (selected) onAccent else text)
        views.setOnClickPendingIntent(id, broadcast(context, ACTION_SET_MODE, mode, i))
      }

      views.setTextViewText(R.id.note, shown.note ?: shown.updatedAt?.let {
        "Updated " + DateUtils.getRelativeTimeSpanString(
          it, System.currentTimeMillis(), DateUtils.MINUTE_IN_MILLIS,
        )
      } ?: "")
      views.setOnClickPendingIntent(R.id.note, if (shown.opensApp) open else null)
      views.setOnClickPendingIntent(R.id.open, open)
      manager.updateAppWidget(ids, views)
    }

    private fun openApp(context: Context): PendingIntent? =
      context.packageManager.getLaunchIntentForPackage(context.packageName)?.let {
        PendingIntent.getActivity(context, OPEN_REQUEST, it, PendingIntent.FLAG_IMMUTABLE)
      }

    private fun broadcast(context: Context, action: String, mode: String?, code: Int): PendingIntent {
      val intent = Intent(context, ModeWidget::class.java).setAction(action).putExtra(EXTRA_MODE, mode)
      return PendingIntent.getBroadcast(
        context, code, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
      )
    }
  }
}

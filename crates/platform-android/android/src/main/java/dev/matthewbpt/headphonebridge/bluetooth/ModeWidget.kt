package dev.matthewbpt.headphonebridge.bluetooth

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.graphics.Typeface
import android.text.SpannableString
import android.text.Spanned
import android.text.style.StyleSpan
import android.text.format.DateUtils
import android.util.Log
import android.view.View
import android.widget.RemoteViews
import androidx.core.content.ContextCompat

/**
 * The "Noise Control" home screen widget. It follows the Primary Device:
 * its name and battery, a round button per Noise Control Mode joined by a
 * track, as on the App Screen, and what the last action did. Tapping a mode
 * sets it, tapping the name refreshes, and "Open" opens the app. Without the
 * Nearby devices permission, every tap opens the app.
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
    /** How faded a Pending mode's circle and label are; RemoteViews can't spin a ring round it. */
    private const val PENDING_ALPHA = 0.6f

    private class SlotViews(val slot: Int, val trackStart: Int, val trackEnd: Int, val circle: Int, val label: Int)

    private val SLOT_VIEWS = listOf(
      SlotViews(R.id.slot0, R.id.track0_start, R.id.track0_end, R.id.circle0, R.id.label0),
      SlotViews(R.id.slot1, R.id.track1_start, R.id.track1_end, R.id.circle1, R.id.label1),
      SlotViews(R.id.slot2, R.id.track2_start, R.id.track2_end, R.id.circle2, R.id.label2),
      SlotViews(R.id.slot3, R.id.track3_start, R.id.track3_end, R.id.circle3, R.id.label3),
    )

    fun updateAll(context: Context) {
      val manager = AppWidgetManager.getInstance(context)
      val ids = manager.getAppWidgetIds(ComponentName(context, ModeWidget::class.java))
      if (ids.isNotEmpty()) render(context, manager, ids)
    }

    private fun render(context: Context, manager: AppWidgetManager, ids: IntArray) {
      val shown = Surfaces.copy(context).shown(hasNearbyDevicesPermission(context))
      val views = RemoteViews(context.packageName, R.layout.mode_widget)
      val open = openApp(context)

      views.setTextViewText(R.id.name, shown.name ?: context.getString(R.string.hb_no_device))
      views.setTextViewText(R.id.battery, shown.battery ?: "")
      views.setOnClickPendingIntent(
        R.id.header,
        if (shown.opensApp) open else broadcast(context, ACTION_REFRESH, null, REFRESH_REQUEST),
      )
      views.setOnClickPendingIntent(R.id.root, if (shown.opensApp) open else null)

      renderModes(context, views, widgetLook(shown))

      views.setTextViewText(R.id.note, shown.note ?: shown.updatedAt?.let {
        "Updated " + DateUtils.getRelativeTimeSpanString(
          it, System.currentTimeMillis(), DateUtils.MINUTE_IN_MILLIS,
        )
      } ?: "")
      views.setOnClickPendingIntent(R.id.note, if (shown.opensApp) open else null)
      views.setOnClickPendingIntent(R.id.open, open)
      manager.updateAppWidget(ids, views)
    }

    private fun renderModes(context: Context, views: RemoteViews, look: WidgetLook) {
      // Invisible rather than gone, so the note stays at the bottom.
      views.setViewVisibility(R.id.modes, if (look.slots.isEmpty()) View.INVISIBLE else View.VISIBLE)
      val accent = ContextCompat.getColor(context, R.color.hb_accent)
      val onAccent = ContextCompat.getColor(context, R.color.hb_on_accent)
      val text = ContextCompat.getColor(context, R.color.hb_text)
      SLOT_VIEWS.forEachIndexed { i, ids ->
        val slot = look.slots.getOrNull(i)
        if (slot == null) {
          views.setViewVisibility(ids.slot, View.GONE)
          return@forEachIndexed
        }
        views.setViewVisibility(ids.slot, View.VISIBLE)
        // The track runs from the first circle's centre to the last's.
        views.setViewVisibility(ids.trackStart, if (i == 0) View.INVISIBLE else View.VISIBLE)
        views.setViewVisibility(ids.trackEnd, if (i == look.slots.lastIndex) View.INVISIBLE else View.VISIBLE)
        views.setImageViewResource(ids.circle, modeIcon(slot.mode) ?: 0)
        views.setInt(
          ids.circle,
          "setBackgroundResource",
          if (slot.filled) R.drawable.mode_selected else R.drawable.mode_unselected,
        )
        views.setInt(ids.circle, "setColorFilter", if (slot.filled) onAccent else text)
        views.setTextViewText(ids.label, if (slot.filled) bold(slot.label) else slot.label)
        views.setTextColor(ids.label, if (slot.filled) accent else text)
        val alpha = if (slot.pending) PENDING_ALPHA else 1f
        views.setFloat(ids.circle, "setAlpha", alpha)
        views.setFloat(ids.label, "setAlpha", alpha)
        views.setContentDescription(ids.slot, slot.label)
        views.setOnClickPendingIntent(ids.slot, broadcast(context, ACTION_SET_MODE, slot.mode, i))
      }
    }

    private fun bold(text: String): CharSequence =
      SpannableString(text).apply { setSpan(StyleSpan(Typeface.BOLD), 0, length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE) }

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

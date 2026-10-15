package dev.matthewbpt.headphonebridge.bluetooth

/** How the widget's modes look. */
data class WidgetLook(val slots: List<Slot>) {
  data class Slot(
    val mode: String,
    val label: String,
    val filled: Boolean,
    val pending: Boolean,
  )
}

/**
 * The widget's modes for what the surfaces show, in the App Screen's order,
 * then any the app doesn't know (a future core value) in Capability order.
 */
fun widgetLook(shown: StoredCopy.Shown): WidgetLook =
  WidgetLook(
    (MODE_ORDER.filter { it in shown.modes } + shown.modes.filter { it !in MODE_ORDER }).map { mode ->
      val filled = mode == shown.mode
      WidgetLook.Slot(mode, modeLabel(mode), filled, pending = filled && shown.pending)
    },
  )

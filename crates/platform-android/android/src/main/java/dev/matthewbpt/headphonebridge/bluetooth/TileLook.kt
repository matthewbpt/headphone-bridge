package dev.matthewbpt.headphonebridge.bluetooth

import androidx.annotation.DrawableRes

/** How the "Noise Control" tile looks, and what tapping it does. */
data class TileLook(
  val label: String,
  val subtitle: String?,
  @DrawableRes val icon: Int,
  val state: State,
  val tap: Tap,
) {
  enum class State { ACTIVE, INACTIVE, UNAVAILABLE }

  sealed class Tap {
    data class SetMode(val mode: String) : Tap()
    object Refresh : Tap()
    object OpenApp : Tap()
    object Nothing : Tap()
  }
}

const val TILE_LABEL = "Noise Control"
@DrawableRes val TILE_ICON = R.drawable.ic_noise_control

/**
 * The tile for what the surfaces show: the Primary Device's Chosen Mode, or
 * the mode an action is setting, and a tap that steps to the next one. As
 * [StoredCopy.Shown.mode] is the pending mode during an action, quick taps
 * step on from it and become one change.
 */
fun tileLook(shown: StoredCopy.Shown): TileLook {
  val subtitle = shown.note ?: shown.name
  if (shown.opensApp) return TileLook(TILE_LABEL, subtitle, TILE_ICON, TileLook.State.INACTIVE, TileLook.Tap.OpenApp)
  if (shown.name == null) return TileLook(TILE_LABEL, subtitle, TILE_ICON, TileLook.State.UNAVAILABLE, TileLook.Tap.Nothing)
  val mode = shown.mode
  val label = mode?.let { modeLabel(it) } ?: TILE_LABEL
  val icon = mode?.let { modeIcon(it) } ?: TILE_ICON
  val state = if (mode != null && mode != "off") TileLook.State.ACTIVE else TileLook.State.INACTIVE
  val next = nextMode(shown.modes, mode)
  return TileLook(label, subtitle, icon, state, next?.let { TileLook.Tap.SetMode(it) } ?: TileLook.Tap.Refresh)
}

/** The mode after [mode] in [modes], wrapping round; the first if [mode] isn't one; null with no modes. */
private fun nextMode(modes: List<String>, mode: String?): String? {
  if (modes.isEmpty()) return null
  return modes[(modes.indexOf(mode) + 1) % modes.size]
}

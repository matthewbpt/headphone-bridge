package dev.matthewbpt.headphonebridge.bluetooth

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class TileLookTest {
  private val buds = listOf("off", "noiseCancelling", "ambient", "adaptive")

  private fun shown(
    mode: String? = "off",
    modes: List<String> = buds,
    name: String? = "Galaxy Buds4 Pro",
    status: String? = null,
    opensApp: Boolean = false,
  ) = StoredCopy.Shown(name, null, modes, mode, 1_000L, status, opensApp)

  @Test
  fun aTapStepsToTheNextModeInCapabilityOrder() {
    assertEquals(TileLook.Tap.SetMode("noiseCancelling"), tileLook(shown("off")).tap)
    assertEquals(TileLook.Tap.SetMode("adaptive"), tileLook(shown("ambient")).tap)
  }

  @Test
  fun aTapFromTheLastModeWrapsToTheFirst() {
    assertEquals(TileLook.Tap.SetMode("off"), tileLook(shown("adaptive")).tap)
    val xm5 = listOf("off", "noiseCancelling", "ambient")
    assertEquals(TileLook.Tap.SetMode("off"), tileLook(shown("ambient", xm5)).tap)
  }

  @Test
  fun aTapWithNoKnownModeStepsToTheFirst() {
    assertEquals(TileLook.Tap.SetMode("off"), tileLook(shown(mode = null)).tap)
    assertEquals(TileLook.Tap.SetMode("off"), tileLook(shown(mode = "somethingNew")).tap)
  }

  @Test
  fun aDeviceWithNoKnownModesRefreshesInstead() {
    val look = tileLook(shown(mode = null, modes = emptyList(), status = TAP_TO_REFRESH))
    assertEquals(TileLook.Tap.Refresh, look.tap)
    assertEquals("Tap to refresh", look.subtitle)
  }

  @Test
  fun theLabelIsTheChosenModeAndTheSubtitleTheDevicesName() {
    val look = tileLook(shown("ambient"))
    assertEquals("Ambient", look.label)
    assertEquals("Galaxy Buds4 Pro", look.subtitle)
  }

  @Test
  fun theSubtitleSaysWhatAnActionIsDoingOrWhyItFailed() {
    assertEquals("Setting Ambient…", tileLook(shown("ambient", status = "Setting Ambient…")).subtitle)
    assertEquals(LINK_DROPPED, tileLook(shown("off", status = LINK_DROPPED)).subtitle)
  }

  @Test
  fun itsHighlightedWhileTheChosenModeIsntOff() {
    assertEquals(TileLook.State.ACTIVE, tileLook(shown("noiseCancelling")).state)
    assertEquals(TileLook.State.INACTIVE, tileLook(shown("off")).state)
  }

  @Test
  fun withNoDeviceConnectedItsUnavailable() {
    val look = tileLook(shown(mode = null, modes = emptyList(), name = null, status = NO_DEVICE_CONNECTED))
    assertEquals(TileLook.State.UNAVAILABLE, look.state)
    assertEquals("Noise Control", look.label)
    assertEquals("No Device Connected", look.subtitle)
    assertEquals(TileLook.Tap.Nothing, look.tap)
  }

  @Test
  fun withThePermissionRefusedATapOpensTheApp() {
    val look = tileLook(shown(mode = null, modes = emptyList(), status = ALLOW_NEARBY_DEVICES, opensApp = true))
    assertEquals(TileLook.Tap.OpenApp, look.tap)
    assertEquals(TileLook.State.INACTIVE, look.state)
    assertEquals("Open the app to allow Nearby devices", look.subtitle)
  }

  @Test
  fun threeQuickTapsMakeOneChangeToTheModeShownLast() {
    val link = FakeCoreLink(chosen = "off").apply { connect() }
    val copy = StoredCopy(MemoryStorage(), now = { 5_000L })
    val controller = ActionController(
      link,
      copy,
      start = { true },
      timeouts = ActionController.Timeouts(attachMs = 300, commandMs = 300, confirmMs = 300),
    )
    copy.onDeviceList(link.deviceList!!)
    link.attaches = false
    val done = CountDownLatch(3)
    val labels = (1..3).map {
      val tap = tileLook(copy.shown(hasPermission = true)).tap as TileLook.Tap.SetMode
      controller.tap(tap.mode) { done.countDown() }
      tileLook(copy.shown(hasPermission = true)).label
    }
    // The label steps with every tap, before the Device is even Attached.
    assertEquals(listOf("Noise Cancelling", "Ambient", "Adaptive"), labels)

    waitFor { "holdPrimary" in link.sent }
    link.attach()
    assertTrue("the taps finished", done.await(5, TimeUnit.SECONDS))
    assertEquals(listOf("holdPrimary", "set adaptive", "endHoldPrimary"), link.sent)
    assertEquals("Adaptive", tileLook(copy.shown(hasPermission = true)).label)
  }
}

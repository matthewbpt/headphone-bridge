package dev.matthewbpt.headphonebridge.bluetooth

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class WidgetLookTest {
  private val buds = listOf("off", "noiseCancelling", "ambient", "adaptive")
  private val xm5 = listOf("off", "noiseCancelling", "ambient")

  // Tests name only the parameters they care about; the rest default.
  @Suppress("LongParameterList")
  private fun shown(
    mode: String? = "off",
    modes: List<String> = buds,
    name: String? = "Galaxy Buds4 Pro",
    note: String? = null,
    opensApp: Boolean = false,
    pending: Boolean = false,
  ) = StoredCopy.Shown(name, null, modes, mode, 1_000L, note, opensApp, pending)

  @Test
  fun theBuds4ProsModesAreInTheAppScreensOrderWhateverTheCapabilityOrder() {
    val expected = listOf("off", "ambient", "adaptive", "noiseCancelling")
    assertEquals(expected, widgetLook(shown()).slots.map { it.mode })
    assertEquals(expected, widgetLook(shown(modes = buds.reversed())).slots.map { it.mode })
  }

  @Test
  fun theXm5ShowsOnlyItsThreeModes() {
    val look = widgetLook(shown(modes = xm5, name = "WH-1000XM5"))
    assertEquals(listOf("off", "ambient", "noiseCancelling"), look.slots.map { it.mode })
  }

  @Test
  fun aModeTheAppDoesntKnowComesAfterTheKnownOnes() {
    val look = widgetLook(shown(mode = "somethingNew", modes = listOf("somethingNew") + xm5))
    assertEquals(listOf("off", "ambient", "noiseCancelling", "somethingNew"), look.slots.map { it.mode })
    assertEquals(listOf("somethingNew"), look.slots.filter { it.filled }.map { it.mode })
  }

  @Test
  fun eachSlotIsLabelledAsTheShellLabelsItsMode() {
    assertEquals(
      listOf("Off", "Ambient", "Adaptive", "Noise Cancelling"),
      widgetLook(shown()).slots.map { it.label },
    )
  }

  @Test
  fun exactlyTheChosenModeIsFilled() {
    val look = widgetLook(shown("adaptive"))
    assertEquals(listOf("adaptive"), look.slots.filter { it.filled }.map { it.mode })
    assertEquals(emptyList<String>(), look.slots.filter { it.pending }.map { it.mode })
  }

  @Test
  fun withNoKnownModeNothingIsFilled() {
    assertEquals(emptyList<String>(), widgetLook(shown(mode = null)).slots.filter { it.filled }.map { it.mode })
  }

  @Test
  fun theModeAnActionIsSettingIsFilledAndPendingUntilItsDone() {
    val (link, copy, controller) = action()
    val done = CountDownLatch(1)
    controller.tap("ambient") { done.countDown() }

    val setting = widgetLook(copy.shown(hasPermission = true))
    assertEquals(listOf("ambient"), setting.slots.filter { it.filled }.map { it.mode })
    assertEquals(listOf("ambient"), setting.slots.filter { it.pending }.map { it.mode })

    waitFor { "holdPrimary" in link.sent }
    link.attach()
    assertTrue("the tap finished", done.await(5, TimeUnit.SECONDS))
    val finished = widgetLook(copy.shown(hasPermission = true))
    assertEquals(listOf("ambient"), finished.slots.filter { it.filled }.map { it.mode })
    assertEquals(emptyList<String>(), finished.slots.filter { it.pending }.map { it.mode })
  }

  @Test
  fun afterAFailedActionTheChosenModeIsFilledAgain() {
    val (_, copy, controller) = action()
    val done = CountDownLatch(1)
    controller.tap("ambient") { done.countDown() }

    // The Device is never Attached, so the action gives up.
    assertTrue("the tap finished", done.await(5, TimeUnit.SECONDS))
    val look = widgetLook(copy.shown(hasPermission = true))
    assertEquals(listOf("off"), look.slots.filter { it.filled }.map { it.mode })
    assertEquals(emptyList<String>(), look.slots.filter { it.pending }.map { it.mode })
  }

  @Test
  fun withNoDeviceConnectedThereAreNoSlots() {
    val look = widgetLook(shown(mode = null, modes = emptyList(), name = null, note = NO_DEVICE_CONNECTED))
    assertEquals(emptyList<WidgetLook.Slot>(), look.slots)
  }

  @Test
  fun withNoKnownModesThereAreNoSlots() {
    val shown = shown(mode = null, modes = emptyList(), note = TAP_TO_REFRESH)
    assertEquals(emptyList<WidgetLook.Slot>(), widgetLook(shown).slots)
  }

  @Test
  fun withThePermissionRefusedThereAreNoSlots() {
    val look = widgetLook(shown(mode = null, modes = emptyList(), note = ALLOW_NEARBY_DEVICES, opensApp = true))
    assertEquals(emptyList<WidgetLook.Slot>(), look.slots)
  }

  /** A Buds4 Pro in Off that won't Attach until the test says so. */
  private fun action(): Triple<FakeCoreLink, StoredCopy, ActionController> {
    val link = FakeCoreLink(chosen = "off").apply { connect() }
    val copy = StoredCopy(MemoryStorage(), now = { 5_000L })
    val controller =
      ActionController(
        link,
        copy,
        start = { true },
        timeouts = ActionController.Timeouts(attachMs = 300, commandMs = 300, confirmMs = 300),
      )
    copy.onDeviceList(link.deviceList!!)
    link.attaches = false
    return Triple(link, copy, controller)
  }
}

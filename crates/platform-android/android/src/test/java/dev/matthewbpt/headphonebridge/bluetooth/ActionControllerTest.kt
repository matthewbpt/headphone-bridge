package dev.matthewbpt.headphonebridge.bluetooth

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class ActionControllerTest {
  private val link = FakeCoreLink().apply { connect() }
  private val copy = StoredCopy(MemoryStorage(), now = { 5_000L })
  private var permitted = true
  private val controller = ActionController(
    link,
    copy,
    start = { permitted },
    timeouts = ActionController.Timeouts(attachMs = 300, commandMs = 300, confirmMs = 300),
  )

  /** Taps [mode], or refresh when null, and waits for the action to finish. */
  private fun tapAndWait(mode: String?) {
    val done = CountDownLatch(1)
    controller.tap(mode) { done.countDown() }
    assertTrue("the action finished", done.await(5, TimeUnit.SECONDS))
  }

  @Test
  fun anActionHoldsThePrimaryDeviceSetsTheModeAndEndsItsHold() {
    tapAndWait("ambient")

    assertEquals(listOf("holdPrimary", "set ambient", "endHoldPrimary"), link.sent)
    val shown = copy.shown(permitted)
    assertEquals("ambient", shown.mode)
    assertEquals(null, shown.status)
  }

  @Test
  fun tapsDuringAnActionChangeItsTargetInsteadOfStartingAnother() {
    link.attaches = false
    val done = CountDownLatch(2)
    controller.tap("ambient") { done.countDown() }
    controller.tap("noiseCancelling") { done.countDown() }

    val shown = copy.shown(permitted)
    assertEquals("noiseCancelling", shown.mode)
    assertEquals("Setting Noise Cancelling…", shown.status)

    waitFor { "holdPrimary" in link.sent }
    link.attach()
    assertTrue("both taps finished", done.await(5, TimeUnit.SECONDS))
    assertEquals(listOf("holdPrimary", "set noiseCancelling", "endHoldPrimary"), link.sent)
    assertEquals("noiseCancelling", copy.shown(permitted).mode)
  }

  @Test
  fun anActionGivesUpWhenTheDeviceIsntAttachedInTime() {
    link.attaches = false

    tapAndWait("ambient")

    assertEquals(listOf("holdPrimary", "endHoldPrimary"), link.sent)
    val shown = copy.shown(permitted)
    assertEquals("Another app or device may be using it", shown.status)
    assertEquals("off", shown.mode)
  }

  @Test
  fun aRejectedSetEndsTheHoldAndSaysSo() {
    link.commandError = "\"notAttached\""

    tapAndWait("ambient")

    assertEquals(listOf("holdPrimary", "set ambient", "endHoldPrimary"), link.sent)
    val shown = copy.shown(permitted)
    assertEquals("Couldn't set Ambient", shown.status)
    assertEquals("off", shown.mode)
  }

  @Test
  fun aSetTheDeviceDoesntConfirmEndsTheHoldAndSaysSo() {
    link.confirms = false

    tapAndWait("ambient")

    assertEquals(listOf("holdPrimary", "set ambient", "endHoldPrimary"), link.sent)
    assertEquals("The Device didn't confirm Ambient", copy.shown(permitted).status)
  }

  @Test
  fun withThePermissionRefusedAnActionHoldsNothingAndSaysWhy() {
    permitted = false

    tapAndWait("ambient")

    assertEquals(emptyList<String>(), link.sent)
    assertEquals("Open the app to allow Nearby devices", copy.shown(permitted).status)
  }

  @Test
  fun aColdStartHoldsThePrimaryDeviceOnceTheCoreReportsIt() {
    link.disconnect()
    val done = CountDownLatch(1)
    controller.tap("ambient") { done.countDown() }

    Thread.sleep(50)
    link.connect()

    assertTrue("the action finished", done.await(5, TimeUnit.SECONDS))
    assertEquals(listOf("set ambient", "endHoldPrimary"), link.sent.takeLast(2))
    assertEquals(null, copy.shown(permitted).status)
  }

  @Test
  fun withNoDeviceConnectedAnActionGivesUpAndSaysSo() {
    link.disconnect()

    tapAndWait("ambient")

    assertEquals(emptyList<String>(), link.sent)
    assertEquals("No Device Connected", copy.shown(permitted).status)
  }

  @Test
  fun aRefreshHoldsThePrimaryDeviceUntilItReportsItsMode() {
    link.chosen = "adaptive"

    tapAndWait(null)

    assertEquals(listOf("holdPrimary", "endHoldPrimary"), link.sent)
    val shown = copy.shown(permitted)
    assertEquals("adaptive", shown.mode)
    assertEquals(null, shown.status)
  }
}

/** Waits up to 5 s for [condition]. */
fun waitFor(condition: () -> Boolean) {
  val deadline = System.currentTimeMillis() + 5000
  while (!condition()) {
    if (System.currentTimeMillis() > deadline) throw AssertionError("timed out waiting")
    Thread.sleep(5)
  }
}

package dev.matthewbpt.headphonebridge.bluetooth

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test

class StoredCopyTest {
  private val storage = MemoryStorage()
  private var now = 10_000L
  private val copy = StoredCopy(storage, now = { now })

  private val buds = listOf("off", "noiseCancelling", "ambient", "adaptive")
  private val budsBattery =
    JSONObject()
      .put("kind", "buds")
      .put("left", 80)
      .put("right", 75)
      .put("case", 50)

  @Test
  fun theCopySurvivesTheProcessBeingKilled() {
    copy.onDeviceList(
      deviceList(device("AA", "Galaxy Buds4 Pro", state("attached"), buds, "ambient", budsBattery)),
    )

    val shown = StoredCopy(storage, now = { 99_000L }).shown(hasPermission = true)

    assertEquals("Galaxy Buds4 Pro", shown.name)
    assertEquals("L 80% · R 75% · Case 50%", shown.battery)
    assertEquals(buds, shown.modes)
    assertEquals("ambient", shown.mode)
    assertEquals(10_000L, shown.updatedAt)
    assertEquals(null, shown.note)
  }

  @Test
  fun aReturningDeviceShowsItsRememberedModes() {
    val xm5 = listOf("off", "noiseCancelling", "ambient")
    copy.onDeviceList(deviceList(device("AA", "Galaxy Buds4 Pro", state("attached"), buds, "ambient")))
    copy.onDeviceList(deviceList(device("BB", "WH-1000XM5", state("attached"), xm5, "off")))
    copy.onDeviceList(deviceList())

    // A new Connected spell, in a new process: the core knows nothing of it yet.
    val again = StoredCopy(storage, now = { now })
    again.onDeviceList(deviceList(device("AA", "Galaxy Buds4 Pro", state("detached"))))

    val shown = again.shown(hasPermission = true)
    assertEquals("Galaxy Buds4 Pro", shown.name)
    assertEquals(buds, shown.modes)
    assertEquals(null, shown.note)
  }

  @Test
  fun aDeviceNeverAttachedShowsTapToRefreshInsteadOfModes() {
    copy.onDeviceList(deviceList(device("CC", "Galaxy Buds4 Pro", state("detached"))))

    val shown = copy.shown(hasPermission = true)
    assertEquals("Galaxy Buds4 Pro", shown.name)
    assertEquals(emptyList<String>(), shown.modes)
    assertEquals("Tap to refresh", shown.note)
  }

  @Test
  fun withNoDeviceConnectedTheCopySaysSo() {
    copy.onDeviceList(deviceList(device("AA", "Galaxy Buds4 Pro", state("attached"), buds, "ambient")))
    copy.onDeviceList(deviceList())

    val shown = copy.shown(hasPermission = true)
    assertEquals(null, shown.name)
    assertEquals(emptyList<String>(), shown.modes)
    assertEquals("No Device Connected", shown.note)
    assertEquals(false, shown.opensApp)
  }

  @Test
  fun withThePermissionRefusedTheCopyAsksToOpenTheApp() {
    copy.onDeviceList(deviceList(device("AA", "Galaxy Buds4 Pro", state("attached"), buds, "ambient")))

    val shown = copy.shown(hasPermission = false)
    assertEquals(emptyList<String>(), shown.modes)
    assertEquals("Open the app to allow Nearby devices", shown.note)
    assertEquals(true, shown.opensApp)
  }

  @Test
  fun valuesKeptWhileDetachedAreDatedWhenTheDeviceReportedThem() {
    copy.onDeviceList(
      deviceList(device("AA", "Galaxy Buds4 Pro", state("detached"), buds, "ambient").put("reportedAt", 4_000L)),
    )

    assertEquals(4_000L, copy.shown(hasPermission = true).updatedAt)
  }

  @Test
  fun anActionCutShortByTheProcessDyingIsntShownAfter() {
    copy.onDeviceList(deviceList(device("AA", "Galaxy Buds4 Pro", state("detached"), buds, "off")))
    copy.actionStarted("ambient")
    copy.onDeviceList(deviceList(device("AA", "Galaxy Buds4 Pro", state("attaching"), buds, "off")))

    val shown = StoredCopy(storage, now = { now }).shown(hasPermission = true)
    assertEquals("off", shown.mode)
    assertEquals(null, shown.note)
  }

  @Test
  fun theModeAnActionIsSettingIsShownAsPendingUntilTheActionIsOver() {
    copy.onDeviceList(deviceList(device("AA", "Galaxy Buds4 Pro", state("attached"), buds, "off")))
    assertEquals(false, copy.shown(hasPermission = true).pending)

    copy.actionStarted("ambient")
    val setting = copy.shown(hasPermission = true)
    assertEquals("ambient", setting.mode)
    assertEquals(true, setting.pending)

    copy.actionFinished(null)
    val finished = copy.shown(hasPermission = true)
    assertEquals("off", finished.mode)
    assertEquals(false, finished.pending)
  }

  @Test
  fun aFailedActionOrARefreshLeavesNothingPending() {
    copy.onDeviceList(deviceList(device("AA", "Galaxy Buds4 Pro", state("attached"), buds, "off")))
    copy.actionStarted("ambient")
    copy.actionFinished("The link dropped")
    assertEquals(false, copy.shown(hasPermission = true).pending)

    copy.actionStarted(null)
    val refreshing = copy.shown(hasPermission = true)
    assertEquals("off", refreshing.mode)
    assertEquals(false, refreshing.pending)
  }

  @Test
  fun whyAnActionFailedIsShownUntilTheDeviceIsAttachedAgain() {
    copy.onDeviceList(deviceList(device("AA", "Galaxy Buds4 Pro", state("detached"), buds, "off")))
    copy.actionStarted("ambient")
    copy.actionFinished("Another app or device may be using it")

    val again = StoredCopy(storage, now = { now })
    assertEquals("Another app or device may be using it", again.shown(hasPermission = true).note)

    again.onDeviceList(deviceList(device("AA", "Galaxy Buds4 Pro", state("attached"), buds, "off")))
    assertEquals(null, again.shown(hasPermission = true).note)
  }
}

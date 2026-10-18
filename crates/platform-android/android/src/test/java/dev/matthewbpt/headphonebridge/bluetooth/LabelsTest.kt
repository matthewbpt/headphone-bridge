package dev.matthewbpt.headphonebridge.bluetooth

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test

class LabelsTest {
  /** The shell's labels, as `NativeCore.labels` hands them across, with made-up words. */
  private val labels =
    labels(
      "inUseByAnotherProcess" to "Words for in use",
      "linkDropped" to "Words for a dropped link",
      "other" to "Words for a link failure without a cause",
    )

  @Test
  fun aReasonIsLabelledAsTheShellLabelsIt() {
    val list = deviceList(device("AA:BB", "WH-1000XM5", notAttached("inUseByAnotherProcess")))

    assertEquals("Words for in use", notAttachedText(list, labels))
  }

  @Test
  fun aReasonNewToTheWidgetIsLabelledAsTheShellLabelsIt() {
    val labels = labels("someNewReason" to "Words for the new reason")
    val list = deviceList(device("AA:BB", "WH-1000XM5", notAttached("someNewReason")))

    assertEquals("Words for the new reason", notAttachedText(list, labels))
  }

  @Test
  fun aLinkFailureWithoutACauseIsLabelledWithItsDetailBeneath() {
    val reason = JSONObject().put("kind", "other").put("detail", "secure: IOException")
    val list = deviceList(device("AA:BB", "WH-1000XM5", state("notAttached").put("reason", reason)))

    assertEquals("Words for a link failure without a cause\nsecure: IOException", notAttachedText(list, labels))
  }

  @Test
  fun aLinkFailureWithoutACauseOrDetailIsLabelledAlone() {
    val reason = JSONObject().put("kind", "other").put("detail", "")
    val list = deviceList(device("AA:BB", "WH-1000XM5", state("notAttached").put("reason", reason)))

    assertEquals("Words for a link failure without a cause", notAttachedText(list, labels))
  }

  @Test
  fun aDeviceThatIsntNotAttachedIsLabelledAsADroppedLink() {
    val list = deviceList(device("AA:BB", "WH-1000XM5", state("detached")))

    assertEquals("Words for a dropped link", notAttachedText(list, labels))
  }

  @Test
  fun noDeviceIsLabelledAsNoneConnected() {
    assertEquals(NO_DEVICE_CONNECTED, notAttachedText(deviceList(), labels))
  }

  @Test
  fun aReasonTheShellHasNoLabelForIsShownByItsName() {
    val list = deviceList(device("AA:BB", "WH-1000XM5", notAttached("someNewReason")))

    assertEquals("someNewReason", notAttachedText(list, labels))
  }

  @Test
  fun withoutTheShellsLabelsAReasonIsShownByItsName() {
    val list = deviceList(device("AA:BB", "WH-1000XM5", notAttached("inUseByAnotherProcess")))

    assertEquals("inUseByAnotherProcess", notAttachedText(list, JSONObject()))
  }
}

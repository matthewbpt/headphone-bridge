package dev.matthewbpt.headphonebridge.bluetooth

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test

class LabelsTest {
  @Test
  fun aLinkFailureWithoutACauseIsLabelledWithItsDetailBeneath() {
    val reason = JSONObject().put("kind", "other").put("detail", "secure: IOException")
    val list = deviceList(device("AA:BB", "WH-1000XM5", state("notAttached").put("reason", reason)))

    assertEquals("Couldn't open the control link\nsecure: IOException", notAttachedText(list))
  }

  @Test
  fun aLinkFailureWithoutACauseOrDetailIsLabelledAlone() {
    val reason = JSONObject().put("kind", "other").put("detail", "")
    val list = deviceList(device("AA:BB", "WH-1000XM5", state("notAttached").put("reason", reason)))

    assertEquals("Couldn't open the control link", notAttachedText(list))
  }

  @Test
  fun aLinkFailureWithACauseIsLabelledAlone() {
    val list = deviceList(device("AA:BB", "WH-1000XM5", notAttached("inUseByAnotherProcess")))

    assertEquals("In use by another app", notAttachedText(list))
  }
}

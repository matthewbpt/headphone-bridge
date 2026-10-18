package dev.matthewbpt.headphonebridge.bluetooth

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.nio.file.Files

class FrameLogSettingTest {
  private val dataDir = Files.createTempDirectory("data").toFile()

  @Test
  fun withTheSettingOffThereIsNoFrameLog() {
    assertNull(frameLogIfOn(dataDir))
  }

  @Test
  fun withTheSettingOnTheFrameLogIsWhereTheShellKeepsIt() {
    File(dataDir, "frame-log-on").writeText("")

    val log = frameLogIfOn(dataDir)

    assertEquals(File(dataDir, "logs/frame-log.frames").path, log)
    assertTrue("its directory exists", File(dataDir, "logs").isDirectory)
  }
}

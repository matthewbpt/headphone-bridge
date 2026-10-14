package dev.matthewbpt.headphonebridge.bluetooth

import java.io.File
import java.nio.file.Files
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SharedFilesTest {
  private val dataDir = Files.createTempDirectory("data").toFile()
  private val cacheDir = Files.createTempDirectory("cache").toFile()

  @Test
  fun sharesACopyInTheSharedCacheDirectory() {
    val log = File(dataDir, "frame-log.frames").apply { writeText("> 01 02\n") }

    val shared = copyForSharing(log, cacheDir)

    assertEquals(File(cacheDir, "$SHARED_DIR/frame-log.frames"), shared)
    assertEquals("> 01 02\n", shared.readText())
  }

  @Test
  fun aLaterShareReplacesTheEarlierCopy() {
    val log = File(dataDir, "frame-log.frames").apply { writeText("> 01\n") }
    copyForSharing(log, cacheDir)
    log.appendText("< 02\n")

    val shared = copyForSharing(log, cacheDir)

    assertEquals("> 01\n< 02\n", shared.readText())
    assertTrue(log.exists())
  }
}

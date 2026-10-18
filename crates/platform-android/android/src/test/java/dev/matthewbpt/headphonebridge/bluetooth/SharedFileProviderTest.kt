package dev.matthewbpt.headphonebridge.bluetooth

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import java.io.File
import java.nio.file.Files

@RunWith(RobolectricTestRunner::class)
// Robolectric's SDK 36 can't start on this JDK (raw FileDescriptor internals).
@Config(sdk = [35])
class SharedFileProviderTest {
  private val context = ApplicationProvider.getApplicationContext<Context>()

  @Test
  fun servesASharedCopyThroughTheManifestsProvider() {
    val log = File(Files.createTempDirectory("data").toFile(), "frame-log.frames")
    log.writeText("> 01 02\n")
    val copy = copyForSharing(log, context.cacheDir)

    val uri = uriForSharing(context, copy)

    assertEquals("content://${context.packageName}.shared/shared/frame-log.frames", uri.toString())
  }
}

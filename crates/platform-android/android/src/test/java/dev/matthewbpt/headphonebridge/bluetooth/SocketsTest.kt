package dev.matthewbpt.headphonebridge.bluetooth

import org.junit.Assert.assertEquals
import org.junit.Test

class SocketsTest {
  @Test
  fun theWh1000xm5IsTriedInsecureFirst() {
    assertEquals(listOf(SocketKind.INSECURE, SocketKind.SECURE), socketKinds(SONY_SERVICE))
    assertEquals(
      listOf(SocketKind.INSECURE, SocketKind.SECURE),
      socketKinds(SONY_SERVICE.uppercase()),
    )
  }

  @Test
  fun theGalaxyBudsAreTriedSecureFirst() {
    assertEquals(listOf(SocketKind.SECURE, SocketKind.INSECURE), socketKinds(SAMSUNG_SERVICE))
  }
}

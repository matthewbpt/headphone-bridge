package dev.matthewbpt.headphonebridge.bluetooth

/** Samsung's vendor control service, on the Galaxy Buds. */
internal const val SAMSUNG_SERVICE = "2e73a4ad-332d-41fc-90e2-16bef06523f2"

/** Sony's vendor control service, on the WH-1000XM5. */
internal const val SONY_SERVICE = "956c7b26-d49a-4ba8-b03f-b17d393cb6e2"

/** The vendor services the core knows, so a Device with one is worth reporting. */
internal val VENDOR_SERVICES = setOf(SAMSUNG_SERVICE, SONY_SERVICE)

/** An RFCOMM socket kind: whether the link is authenticated and encrypted. */
internal enum class SocketKind { SECURE, INSECURE }

/**
 * The socket kinds to try, in order, for a control link to [service]. The
 * WH-1000XM5's first secure connect has always failed, once after 6.3 s, so
 * it gets an insecure one first.
 */
internal fun socketKinds(service: String): List<SocketKind> =
  if (service.equals(SONY_SERVICE, ignoreCase = true)) {
    listOf(SocketKind.INSECURE, SocketKind.SECURE)
  } else {
    listOf(SocketKind.SECURE, SocketKind.INSECURE)
  }

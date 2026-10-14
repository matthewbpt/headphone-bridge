# The core owns Detaching, so the app can share a Device's control link

A Device's vendor service accepts one control link at a time, across every app on a host and every host it's connected to. A new connect knocks off whichever link is open, and the Android spikes (`docs/spikes/android-rfcomm.md`) showed that a client that never stops retrying locks every other client out. On Android the app must therefore Detach whenever nobody is using it. We put that in the core: the core knows Detached as a Device state, Attaches and Detaches when a platform asks, and picks the Primary Device across Connected Devices. The alternative was having the platform adapter hide Devices from the core, which the spike did. Desktop keeps Attaching every Connected Device for as long as it's Connected; the same mechanism is there when desktop needs to Detach for a phone over multipoint.

## Considered Options

- **The platform hides Devices from the core** (spike 2's Kotlin gate): no core change, but the Device list can't show a Detached Device, and the Primary Device logic would exist twice, once in the core and once per platform.

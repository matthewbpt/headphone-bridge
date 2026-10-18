# The Simulated Device wraps any platform adapter, from its own crate

The Simulated Device began inside the Android adapter, which intercepted its address before asking Kotlin for a control link, so only debug Android builds had it. Nothing in it is Android's: it is the Device's end of a control link. So it lives in its own crate, `headphone-bridge-simulated`, as an adapter that implements `Platform` around another adapter: it answers the core for the simulated address, adds the simulated Device's observations to the OS's, and passes every other address through. Every debug build, desktop or Android, wraps its adapter in it; release builds don't. The fixtures it starts from live in a `headphone-bridge-fixtures` crate that the core's tests read too, so neither crate reaches into another's `tests/`.

## Considered Options

- **A `simulated` feature on the core**: one crate fewer, but Cargo can't tie a feature to the debug profile, so release builds would turn it on too, and the core would carry the Device's end of each Vendor protocol beside the app's.
- **A branch in each adapter**, as Android had: the same interception written three times, each with its own state behind `#[cfg(debug_assertions)]`.
- **The core knowing Simulated Devices**: it would need the Device's end of each protocol, which the platform port exists to keep out.

## Consequences

- The simulator speaks the protocols through `core::wire`, the core's framing and message ids. Whatever it needs is exported there, under the names the core's own parsers use, rather than re-derived.
- The crate is a plain dependency of the shell and the Android adapter, because Cargo has no debug-only dependency. Only the `#[cfg(debug_assertions)]` call sites decide whether a build uses it.

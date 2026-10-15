# On Android the core lives in the app's process, outside Tauri

On Android the home screen widget and the Quick Settings tile must work when the app's window isn't open, and their taps can start the process with no Activity. Tauri starts its Rust app, and so anything it hosts, only when the Activity is created. So the core is started by whichever comes first: Kotlin code that owns Bluetooth (reached over plain JNI with JSON messages), or the Tauri setup. The window, widget and tile share the one core per process. Tauri on Android provides the window and its React UI, and its plugin bridge carries only what needs the Activity: asking for and checking the Nearby devices permission, opening the app's page in Android's settings, and the share sheet. Bluetooth and the core's messages don't go through it.

## Considered Options

- **Tauri's mobile plugin bridge** (`run_mobile_plugin_async` and `ipc::Channel`), as in spike 1: it works for the window, but it needs the Activity, so the widget and tile couldn't reach the core.
- **Reimplementing the vendor protocols in Kotlin for the widget and tile**: duplicates what ADR-0001 keeps in Rust.

## Consequences

- The Tauri plugin's Kotlin `load()` runs after the Rust setup, so neither side can assume the other has started the core.

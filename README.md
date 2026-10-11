# Headphone Bridge

A desktop app for macOS and Linux that changes the Noise Control Mode and
Ambient Level of a Galaxy Buds4 Pro or WH-1000XM5, and shows their Status.
Vocabulary follows [GLOSSARY.md](GLOSSARY.md); decisions are in
[docs/adr](docs/adr).

## Layout

- `crates/core`: the platform-independent core. Its public API (`Core`) takes
  commands and emits Device list snapshots; everything OS-specific comes in
  through the platform port (`platform::Platform`). Tests drive it through a
  scripted fake platform in `crates/core/tests/support`.
- `crates/platform-macos`: the macOS platform adapter (IOBluetooth). It
  compiles to nothing on other OSes and is tested by hand.
- `crates/platform-linux`: the Linux platform adapter (BlueZ through `bluer`,
  and the PulseAudio/PipeWire default sink). Its pure mappings build and are
  tested on every OS; the rest compiles only on Linux and is tested by hand.
- `app/src-tauri`: the Tauri v2 shell.
- `app/src`: the React + TypeScript UI.

## Development

Requires Rust (stable) and Node 22.

```sh
cargo test --workspace --exclude headphone-bridge   # core tests, as CI runs them
cd app && npm install
```

### Running on macOS

macOS grants Bluetooth access only to a signed `.app` whose Info.plist has
`NSBluetoothAlwaysUsageDescription`, so a bare `cargo run` or `npm run tauri dev`
starts but can never open a control link. Run development builds through the
bundle instead:

```sh
cd app
npm run open:dev
```

This builds a debug, ad-hoc signed `Headphone Bridge.app` at
`target/debug/bundle/macos/` and opens it. Use `npm run bundle:dev` to build
without opening. `npm run tauri dev` is still fine for UI-only work.

### Running on Linux

Needs no root or group membership: BlueZ opens the control link and hands it
over D-Bus. Install the build dependencies (Ubuntu 24.04):

```sh
sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev \
  libxdo-dev libssl-dev libdbus-1-dev libpulse-dev pkg-config file
```

Then `cd app && npm run tauri dev` runs the app, and
`npm run tauri build -- --bundles deb,appimage` builds the deb and AppImage
under `target/release/bundle/`. On GNOME the tray needs the AppIndicator
extension, which Ubuntu enables by default.

### Frame log

The Developer section of the window switches on the frame log, which appends
every frame sent to or received from a Device to `frame-log.frames` in the
app's log directory, in the test fixture format. The setting stays on across
launches, so Devices already Connected at startup have their Attach logged.
Setting `HEADPHONE_BRIDGE_FRAME_LOG` (to anything) also starts the app with it
on, e.g. `HEADPHONE_BRIDGE_FRAME_LOG=1 "target/debug/bundle/macos/Headphone
Bridge.app/Contents/MacOS/headphone-bridge"`.

## Licence

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your
option. See [NOTICE](NOTICE) for the projects this one ports from.

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

## Licence

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your
option. See [NOTICE](NOTICE) for the projects this one ports from.

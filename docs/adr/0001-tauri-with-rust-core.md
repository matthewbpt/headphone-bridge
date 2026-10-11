# Tauri with a Rust core, because Node can't do RFCOMM

Both target Devices (Galaxy Buds4 Pro, WH-1000XM5) are controlled only over Bluetooth Classic RFCOMM, with no BLE/GATT control path. No maintained Node library supports RFCOMM on macOS: `bluetooth-serial-port` is deprecated by its maintainer, its macOS support is experimental, and it can't look up a service by vendor UUID. Web Serial over Bluetooth is unverified in Electron. So the Bluetooth layer, the vendor protocols and the device model live in Rust, and the UI is TypeScript (React) inside a Tauri webview. Tauri also provides the signed `.app` bundle and Info.plist that macOS requires before it grants Bluetooth permission.

## Considered Options

- **Electron + Node**: rejected because no usable RFCOMM library exists for macOS.
- **Electron UI + Rust native addon (napi-rs)**: the Rust work is the same as with Tauri, plus Electron's size and the burden of native-addon rebuilds.
- **Pure Rust UI (egui/iced/Slint)**: workable, but it would mean learning an unfamiliar UI toolkit just to avoid a small TypeScript layer.

## Consequences

- RFCOMM is written as a thin adapter per OS, against IOBluetooth via `objc2-io-bluetooth` on macOS and `bluer` on Linux, following the approach of the `mw75` crate.
- On macOS, a bare `cargo run` cannot open RFCOMM. Development runs have to go through a bundled, signed `.app`.

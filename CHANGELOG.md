# Changelog

## [1.1.0](https://github.com/matthewbpt/headphone-bridge/compare/v1.0.0...v1.1.0) (2026-10-18)


### Features

* give startup failures context and app settings a typed error ([#116](https://github.com/matthewbpt/headphone-bridge/issues/116)) ([8d1afc2](https://github.com/matthewbpt/headphone-bridge/commit/8d1afc21a5a61ecab4c5b2596173724b8bbd9a60))
* label the widget's Not Attached reasons from the shell ([#119](https://github.com/matthewbpt/headphone-bridge/issues/119)) ([2b898b9](https://github.com/matthewbpt/headphone-bridge/commit/2b898b9f94f1e9eb163565ba1b94cc58e965b29c))
* name four recurring link failures and label them ([#115](https://github.com/matthewbpt/headphone-bridge/issues/115)) ([ca09861](https://github.com/matthewbpt/headphone-bridge/commit/ca098610babdb197184150ba73bcde9894f4a4a7))
* send a typed reason when Android can't open a control link ([#112](https://github.com/matthewbpt/headphone-bridge/issues/112)) ([d823350](https://github.com/matthewbpt/headphone-bridge/commit/d8233509bde5cf13411e00704fdb3fa57b74a011))
* switch on the Simulated Device in every debug build ([#133](https://github.com/matthewbpt/headphone-bridge/issues/133)) ([a41b961](https://github.com/matthewbpt/headphone-bridge/commit/a41b9617c392edc577e6ea61289fe1111f9f17ca))


### Bug Fixes

* **linux:** find Devices when bluetoothd starts after the app ([#117](https://github.com/matthewbpt/headphone-bridge/issues/117)) ([b4a3b3f](https://github.com/matthewbpt/headphone-bridge/commit/b4a3b3fd43a0a98a3b91f0fa3b1efcaf1dfc28f2))

## [1.0.0](https://github.com/matthewbpt/headphone-bridge/compare/v0.3.0...v1.0.0) (2026-10-18)


### ⚠ BREAKING CHANGES

* `MacBluetooth::start` returns `io::Result`, `Message::to_json` returns `serde_json::Result`, and `CommandError` gains `CoreStopped`, which the UI must handle.

### Features

* Equalizer Presets, Bands and Clear Bass ([#58](https://github.com/matthewbpt/headphone-bridge/issues/58)) ([8de2796](https://github.com/matthewbpt/headphone-bridge/commit/8de27966733eca72aa16a89711b6e62778ca0c58))


### Bug Fixes

* return errors instead of panicking in production code ([#96](https://github.com/matthewbpt/headphone-bridge/issues/96)) ([8024f25](https://github.com/matthewbpt/headphone-bridge/commit/8024f25201d950175545b545de689c3b6b1afe77))

## [0.3.0](https://github.com/matthewbpt/headphone-bridge/compare/v0.2.0...v0.3.0) (2026-10-17)


### Features

* support the Galaxy Buds2 ([#91](https://github.com/matthewbpt/headphone-bridge/issues/91)) ([df36c2b](https://github.com/matthewbpt/headphone-bridge/commit/df36c2bd462cb8aa568aebb5753437e25f7ac345))


### Bug Fixes

* share the frame log on Android through the provider's meta-data ([#92](https://github.com/matthewbpt/headphone-bridge/issues/92)) ([d5765d4](https://github.com/matthewbpt/headphone-bridge/commit/d5765d45f36365d3629e8a026c49d32b4e5d1f3e))

## [0.2.0](https://github.com/matthewbpt/headphone-bridge/compare/v0.1.0...v0.2.0) (2026-10-15)


### Features

* make the widget's modes look like the App Screen's Modes card ([#85](https://github.com/matthewbpt/headphone-bridge/issues/85)) ([1db87f3](https://github.com/matthewbpt/headphone-bridge/commit/1db87f30c1c33ca80eb422a6a97ef0e6f28561bb))


### Bug Fixes

* **ci:** attach the Linux bundles, not their folders, to the Release ([#83](https://github.com/matthewbpt/headphone-bridge/issues/83)) ([b18432a](https://github.com/matthewbpt/headphone-bridge/commit/b18432a60d4bc1f38065ec28ccbc461b83d21ca2))

## 0.1.0 (2026-10-15)


### Features

* show the Chosen Mode's icon on the Quick Settings tile ([351075d](https://github.com/matthewbpt/headphone-bridge/commit/351075d3b03df66c6201d2aaa0f3c08c94170f48))
* show the Chosen Mode's icon on the Quick Settings tile ([4b1b3ed](https://github.com/matthewbpt/headphone-bridge/commit/4b1b3ed949008a6d55d66d4b75e7dbbfabdfd1d9)), closes [#78](https://github.com/matthewbpt/headphone-bridge/issues/78)

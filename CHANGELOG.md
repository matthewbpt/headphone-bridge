# Changelog

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

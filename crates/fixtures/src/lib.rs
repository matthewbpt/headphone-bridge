//! Recorded control-link frames, in the frame record format the core's
//! `frame_log` parses. `README.md` beside this crate describes the format and
//! where each fixture came from.
//!
//! The core's tests read any fixture from [`DIR`] at runtime. The Simulated
//! Device embeds the few it starts from, below, so it needs no files.

/// The directory holding every fixture, as `<model>/<name>.frames`.
pub const DIR: &str = env!("CARGO_MANIFEST_DIR");

/// A Galaxy Buds4 Pro capture whose first extended status is L 100 %,
/// R 97 %, Ambient.
pub const BUDS4PRO_CAPTURE: &str = include_str!("../galaxy-buds4-pro/capture-3.frames");

/// A Galaxy Buds2's synthesised Attach.
pub const BUDS2_ATTACH: &str = include_str!("../galaxy-buds2/attach.frames");

/// A WH-1000XM5's synthesised Attach.
pub const XM5_ATTACH: &str = include_str!("../wh-1000xm5/attach.frames");

/// A real WH-1000XM5 stepping through its Equalizer Presets.
pub const XM5_EQUALIZER_PRESETS: &str = include_str!("../wh-1000xm5/real-equalizer-presets.frames");

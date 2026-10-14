//! The Android platform adapter: one process-wide core, shared by the app
//! screen, the home screen widget and the Quick Settings tile (ADR-0005).
//!
//! Kotlin owns Bluetooth, and the two sides talk in JSON [`messages`] over
//! JNI. Everything but the JNI glue is plain Rust, built and tested on every
//! OS.

pub mod messages;

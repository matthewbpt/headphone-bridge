//! Commands: the core's only input from the shell.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::platform::DeviceAddress;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    SetNoiseControlMode(DeviceAddress, NoiseControlMode),
    /// Sets the Ambient Level, in the Model's own steps.
    SetAmbientLevel(DeviceAddress, u8),
    /// Tries again to Attach a Connected Device that isn't Attached.
    RetryAttach(DeviceAddress),
    /// Switches the frame log on, appending to the given file, or off (`None`).
    SetFrameLog(Option<PathBuf>),
}

/// Why a command was rejected before anything was sent to a Device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CommandError {
    /// No Connected Device has that address.
    UnknownDevice,
    /// The frame log file couldn't be opened.
    FrameLogUnavailable(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoiseControlMode {
    Off,
    NoiseCancelling,
    Ambient,
    Adaptive,
}

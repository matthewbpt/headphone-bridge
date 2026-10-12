//! Commands: the core's only input from the shell.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::platform::DeviceAddress;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    SetNoiseControlMode(DeviceAddress, NoiseControlMode),
    /// Sets the Ambient Level, in the Model's own steps.
    SetAmbientLevel(DeviceAddress, u8),
    /// Sets the Noise Cancelling Level, in the Model's own steps.
    SetNoiseCancellingLevel(DeviceAddress, u8),
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
    /// The Device is Connected but not Attached, so the app can't change it.
    NotAttached,
    /// The Device's Model isn't on the allowlist, so the app won't control it.
    UnsupportedModel,
    /// The Device's Model doesn't have the Capability the command needs.
    MissingCapability,
    /// The value is outside the range the Model's Capabilities declare.
    OutOfRange,
    /// The Device is Attached but hasn't reported its Noise Control Mode and
    /// Ambient Level yet, so a set couldn't keep the other unchanged.
    SettingsNotReported,
    /// The frame log file couldn't be opened.
    FrameLogUnavailable(String),
}

impl CommandError {
    pub(crate) fn frame_log_unavailable(error: std::io::Error) -> CommandError {
        CommandError::FrameLogUnavailable(error.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoiseControlMode {
    Off,
    NoiseCancelling,
    Ambient,
    Adaptive,
}

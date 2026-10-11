//! Commands: the core's only input from the shell.

use serde::{Deserialize, Serialize};

use crate::platform::DeviceAddress;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    SetNoiseControlMode(DeviceAddress, NoiseControlMode),
    /// Sets the Ambient Level, in the Model's own steps.
    SetAmbientLevel(DeviceAddress, u8),
    /// Tries again to Attach a Connected Device that isn't Attached.
    RetryAttach(DeviceAddress),
}

/// Why a command was rejected before anything was sent to a Device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CommandError {
    /// No Connected Device has that address.
    UnknownDevice,
    /// The Device is Connected but not Attached, so the app can't change it.
    NotAttached,
    /// The Device's Model doesn't have the Capability the command needs.
    MissingCapability,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoiseControlMode {
    Off,
    NoiseCancelling,
    Ambient,
    Adaptive,
}

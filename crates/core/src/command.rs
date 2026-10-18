//! Commands: the core's only input from the shell.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::model::PresetId;
use crate::platform::DeviceAddress;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    SetNoiseControlMode(DeviceAddress, NoiseControlMode),
    /// Sets the Ambient Level, in the Model's own steps.
    SetAmbientLevel(DeviceAddress, u8),
    /// Sets the Noise Cancelling Level, in the Model's own steps.
    SetNoiseCancellingLevel(DeviceAddress, u8),
    /// Sets the whole Equalizer: a Preset, and its Bands and Clear Bass where
    /// the Model has them.
    SetEqualizer(DeviceAddress, Equalizer),
    /// Powers Off the Device: it switches off and stops being Connected.
    PowerOff(DeviceAddress),
    /// Tries again to Attach a Connected Device that isn't Attached.
    RetryAttach(DeviceAddress),
    /// Switches the frame log on, appending to the given file, or off (`None`).
    SetFrameLog(Option<PathBuf>),
}

/// Why a command was rejected before anything was sent to a Device. Its
/// `Display` is for logs; the shell words it for the listener.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(rename_all = "camelCase")]
pub enum CommandError {
    /// No Connected Device has that address.
    #[error("no Connected Device has that address")]
    UnknownDevice,
    /// The Device is Connected but not Attached, so the app can't change it.
    #[error("the Device isn't Attached")]
    NotAttached,
    /// The Device's Model isn't on the allowlist, so the app won't control it.
    #[error("the Device's Model isn't supported")]
    UnsupportedModel,
    /// The Device is Powering Off, so it takes no more commands.
    #[error("the Device is Powering Off")]
    PoweringOff,
    /// The Device's Model doesn't have the Capability the command needs.
    #[error("the Device's Model lacks the Capability")]
    MissingCapability,
    /// The value is outside the range the Model's Capabilities declare.
    #[error("the value is out of the Model's range")]
    OutOfRange,
    /// The Preset isn't one of the Model's, or is one the app can't pick.
    #[error("the Preset isn't available")]
    UnavailablePreset,
    /// Bands or Clear Bass were given for a Preset whose curve can't be
    /// changed.
    #[error("the Preset's curve is fixed")]
    FixedPreset,
    /// Only one of Bands and Clear Bass was given, for a Model that takes
    /// them together.
    #[error("Bands and Clear Bass must be given together")]
    IncompleteEqualizer,
    /// The Device is Attached but hasn't reported its Noise Control Mode and
    /// Ambient Level yet, so a set couldn't keep the other unchanged. Or it
    /// hasn't reported its Equalizer yet.
    #[error("the Device hasn't reported its settings yet")]
    SettingsNotReported,
    /// The Device is Equalizer Unavailable: it refuses Equalizer changes for
    /// now.
    #[error("the Device is Equalizer Unavailable")]
    EqualizerUnavailable,
    /// The frame log file couldn't be opened.
    #[error("couldn't open the frame log: {0}")]
    FrameLogUnavailable(String),
    /// The core has stopped, so it takes no more commands.
    #[error("the core has stopped")]
    CoreStopped,
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

/// An Equalizer's settings, as reported by a Device or asked of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Equalizer {
    /// A Device may report a Preset its Model's Capability doesn't list,
    /// which is kept as it is.
    pub preset: PresetId,
    /// Each Band's value in the Model's own steps, lowest Band first. `None`
    /// when the Device doesn't report them, or a set of the Preset alone,
    /// which leaves its curve to the Device.
    pub bands: Option<Vec<i8>>,
    /// In the Model's own steps. `None` as for the Bands. A Model with both
    /// is set with both or neither.
    pub clear_bass: Option<i8>,
}

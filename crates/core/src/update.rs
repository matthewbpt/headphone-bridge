//! What a control link session reports to the core, whatever the vendor.

use crate::command::{Equalizer, NoiseControlMode};
use crate::device_list::{Battery, EffectiveModeReason};
use crate::model::Model;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Update {
    Identified(Model),
    /// The Device isn't a supported Model: the session must end without
    /// writing anything else.
    Unsupported,
    Battery(Battery),
    /// The listener chose this mode, and the Device is applying it.
    ChosenMode(NoiseControlMode),
    /// The Device is applying this mode for now, whatever the listener chose,
    /// and why, if it says.
    EffectiveMode(NoiseControlMode, Option<EffectiveModeReason>),
    /// In the Model's own steps, within its Capabilities' range.
    AmbientLevel(u8),
    /// In the Model's own steps, within its Capabilities' range.
    NoiseCancellingLevel(u8),
    /// The Equalizer the Device is applying. Its Preset may be one the
    /// Model's Capability doesn't list.
    Equalizer(Equalizer),
    /// The Device acknowledged a request to Power Off. It should stop being
    /// Connected soon after.
    PowerOffAcknowledged,
}

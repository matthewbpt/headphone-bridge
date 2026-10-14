//! Every label the tray and the window show for a Device's state and
//! settings. The window gets them from the `labels` command, so the two
//! can't drift apart.

use std::collections::HashMap;

use headphone_bridge_core::{EffectiveModeReason, Model, NoiseControlMode, NotAttachedReason};
use serde::Serialize;

pub const NO_DEVICE: &str = "No Device";

pub const POWER_OFF: &str = "Power Off";
pub const POWERING_OFF: &str = "Powering Off…";

pub fn mode(mode: NoiseControlMode) -> &'static str {
    match mode {
        NoiseControlMode::Off => "Off",
        NoiseControlMode::NoiseCancelling => "Noise Cancelling",
        NoiseControlMode::Ambient => "Ambient",
        NoiseControlMode::Adaptive => "Adaptive",
    }
}

/// Why the Device isn't applying the Chosen Mode.
pub fn hint(reason: EffectiveModeReason) -> &'static str {
    match reason {
        EffectiveModeReason::BudOutOfEar => "Paused: an earbud is out of the ear",
        EffectiveModeReason::Unknown => "Paused by the Device",
    }
}

/// Why a Device isn't Attached.
pub fn reason(reason: &NotAttachedReason) -> String {
    match reason {
        NotAttachedReason::UnsupportedModel => "Unsupported Model".into(),
        NotAttachedReason::InUseByAnotherProcess => "In use by another app".into(),
        NotAttachedReason::BluetoothPermissionDenied => {
            "Bluetooth permission denied: enable it in System Settings › Privacy & Security › Bluetooth".into()
        }
        NotAttachedReason::ServiceNotFound => "Control service not found".into(),
        NotAttachedReason::DeviceNotConnected => "Not Connected".into(),
        NotAttachedReason::LinkDropped => "Another app or device may be using it".into(),
        NotAttachedReason::NoResponse => "The Device didn't respond".into(),
        NotAttachedReason::Other(message) => message.clone(),
    }
}

const MODES: [NoiseControlMode; 4] = [
    NoiseControlMode::Off,
    NoiseControlMode::NoiseCancelling,
    NoiseControlMode::Ambient,
    NoiseControlMode::Adaptive,
];

const MODELS: [Model; 2] = [Model::GalaxyBuds4Pro, Model::Wh1000Xm5];

const HINTS: [EffectiveModeReason; 2] = [
    EffectiveModeReason::BudOutOfEar,
    EffectiveModeReason::Unknown,
];

/// Every reason but `Other`, whose label is its own message.
const REASONS: [NotAttachedReason; 7] = [
    NotAttachedReason::UnsupportedModel,
    NotAttachedReason::InUseByAnotherProcess,
    NotAttachedReason::BluetoothPermissionDenied,
    NotAttachedReason::ServiceNotFound,
    NotAttachedReason::DeviceNotConnected,
    NotAttachedReason::LinkDropped,
    NotAttachedReason::NoResponse,
];

/// The labels, as the window gets them.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Labels {
    no_device: &'static str,
    modes: HashMap<NoiseControlMode, &'static str>,
    models: HashMap<Model, &'static str>,
    hints: HashMap<EffectiveModeReason, &'static str>,
    reasons: Vec<ReasonLabel>,
}

#[derive(Debug, Serialize)]
struct ReasonLabel {
    reason: NotAttachedReason,
    text: String,
}

pub fn all() -> Labels {
    Labels {
        no_device: NO_DEVICE,
        modes: MODES.into_iter().map(|m| (m, mode(m))).collect(),
        models: MODELS.into_iter().map(|m| (m, m.display_name())).collect(),
        hints: HINTS.into_iter().map(|r| (r, hint(r))).collect(),
        reasons: REASONS
            .into_iter()
            .map(|r| ReasonLabel {
                text: reason(&r),
                reason: r,
            })
            .collect(),
    }
}

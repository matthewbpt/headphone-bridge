//! The Device list snapshot: the core's only output.

use serde::Serialize;

use crate::command::NoiseControlMode;
use crate::model::{Capabilities, Model};
use crate::platform::{DeviceAddress, LinkFailure};

/// A full snapshot of every Device the core knows about. A new one is emitted
/// on every change.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceList {
    pub devices: Vec<Device>,
    /// The Primary Device: the Attached Device the OS is playing audio
    /// through or, if it isn't playing through one, the most recently Attached
    /// Device. `None` while no Device is Attached.
    pub primary: Option<DeviceAddress>,
}

/// A Connected Device of a vendor the app knows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub address: DeviceAddress,
    /// The name the OS reports, which the listener may have changed.
    pub name: String,
    /// `None` until the Model is identified, and for unsupported Models.
    pub model: Option<Model>,
    pub state: DeviceState,
    pub capabilities: Capabilities,
    /// The Noise Control Mode the listener last selected, by any means.
    /// `None` until the Device first reports it.
    pub chosen_mode: Option<NoiseControlMode>,
    /// The Noise Control Mode the Device is applying right now: usually the
    /// Chosen Mode, but it can differ for a while, e.g. while an earbud is out
    /// of the ear. `None` until the Device first reports it.
    pub effective_mode: Option<NoiseControlMode>,
    /// Why the Effective Mode differs from the Chosen Mode. `None` while they
    /// agree.
    pub effective_mode_reason: Option<EffectiveModeReason>,
    /// The Ambient Level in the Model's own steps, within
    /// `capabilities.ambient_level`. `None` until the Device first reports it.
    pub ambient_level: Option<u8>,
    /// `None` until the Device first reports it.
    pub battery: Option<Battery>,
}

impl Device {
    /// Records the mode the Device is applying, and why, if it says. The
    /// reason shows only while the mode differs from the Chosen Mode.
    pub(crate) fn set_effective_mode(
        &mut self,
        mode: NoiseControlMode,
        reason: Option<EffectiveModeReason>,
    ) {
        self.effective_mode = Some(mode);
        self.effective_mode_reason = (self.chosen_mode.is_some() && self.chosen_mode != Some(mode))
            .then(|| reason.unwrap_or(EffectiveModeReason::Unknown));
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DeviceState {
    /// Opening the control link and identifying the Model.
    Attaching,
    Attached,
    /// Connected, but the app has no control link.
    NotAttached {
        reason: NotAttachedReason,
    },
}

/// Why a Connected Device isn't Attached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "message", rename_all = "camelCase")]
pub enum NotAttachedReason {
    /// The Model isn't on the allowlist, so the app won't control it.
    UnsupportedModel,
    InUseByAnotherProcess,
    BluetoothPermissionDenied,
    ServiceNotFound,
    DeviceNotConnected,
    /// The control link closed while Attached.
    LinkDropped,
    /// The control link opened, but the Device didn't identify itself in time.
    NoResponse,
    Other(String),
}

impl From<LinkFailure> for NotAttachedReason {
    fn from(failure: LinkFailure) -> Self {
        match failure {
            LinkFailure::InUseByAnotherProcess => NotAttachedReason::InUseByAnotherProcess,
            LinkFailure::BluetoothPermissionDenied => NotAttachedReason::BluetoothPermissionDenied,
            LinkFailure::ServiceNotFound => NotAttachedReason::ServiceNotFound,
            LinkFailure::DeviceNotConnected => NotAttachedReason::DeviceNotConnected,
            LinkFailure::Other(message) => NotAttachedReason::Other(message),
        }
    }
}

/// Why a Device's Effective Mode differs from its Chosen Mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EffectiveModeReason {
    /// An earbud is out of the ear, and the Device paused the Chosen Mode.
    BudOutOfEar,
    /// The Device didn't say why.
    Unknown,
}

/// Battery charge in percent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Battery {
    /// Earbuds. A part is `None` when its level is unknown, e.g. a bud that
    /// isn't connected, or a case with no bud docked in it.
    Buds {
        left: Option<u8>,
        right: Option<u8>,
        case: Option<u8>,
    },
    /// Over-ear headphones, with a single battery.
    Headphones { level: u8 },
}

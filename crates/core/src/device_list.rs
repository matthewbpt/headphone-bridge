//! The Device list snapshot: the core's only output.

use serde::Serialize;

use crate::command::{CommandError, NoiseControlMode};
use crate::model::{Capabilities, Model};
use crate::platform::{DeviceAddress, LinkFailure};

/// A full snapshot of every Device the core knows about. A new one is emitted
/// on every change.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceList {
    pub devices: Vec<Device>,
    /// The Primary Device: the Connected Device the OS is playing audio
    /// through or, if it isn't playing through one, the most recently Attached
    /// or shown Device, whether or not it's Attached. Never an unsupported
    /// Model. `None` while no such Device is Connected.
    pub primary: Option<DeviceAddress>,
}

/// A Connected Device of a vendor the app knows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub address: DeviceAddress,
    /// The name the OS reports, which the listener may have changed.
    pub name: String,
    /// `None` until the Model is identified, and for unsupported Models. Kept
    /// while the Device is Attached again, as long as it stays Connected.
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
    /// The Noise Cancelling Level in the Model's own steps, within
    /// `capabilities.noise_cancelling_level`. `None` until the Device first
    /// reports it.
    pub noise_cancelling_level: Option<u8>,
    /// `None` until the Device first reports it.
    pub battery: Option<Battery>,
    /// The Device acknowledged the last Power Off but was still Connected
    /// well after. Cleared when the Device is asked to Power Off again.
    pub power_off_failed: bool,
    /// When a Detached Device last reported the values it keeps, in
    /// milliseconds since the Unix epoch: they may be out of date. `None`
    /// while the values are current, or there are none.
    pub reported_at: Option<u64>,
}

impl Device {
    /// A Device whose Model isn't known, nor anything it reports: when it is
    /// first Connected.
    pub(crate) fn unidentified(address: DeviceAddress, name: String, state: DeviceState) -> Device {
        Device {
            address,
            name,
            model: None,
            state,
            capabilities: Capabilities::default(),
            chosen_mode: None,
            effective_mode: None,
            effective_mode_reason: None,
            ambient_level: None,
            noise_cancelling_level: None,
            battery: None,
            power_off_failed: false,
            reported_at: None,
        }
    }

    /// This Device once it lost its control link, in `state`. What it
    /// reported is dropped,
    /// but it keeps its Model and Capabilities: it's the same Device while it
    /// stays Connected. Commands are refused by its state, not its
    /// Capabilities, until it is Attached again.
    pub(crate) fn detached(&self, state: DeviceState) -> Device {
        Device {
            model: self.model,
            capabilities: self.capabilities.clone(),
            ..Device::unidentified(self.address.clone(), self.name.clone(), state)
        }
    }

    /// Forgets what the Device reported, e.g. values kept from before it was
    /// Detached, once it's Attached again and reports them afresh.
    pub(crate) fn forget_reported(&mut self) {
        self.chosen_mode = None;
        self.effective_mode = None;
        self.effective_mode_reason = None;
        self.ambient_level = None;
        self.noise_cancelling_level = None;
        self.battery = None;
        self.reported_at = None;
    }

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

    /// Fails until the Device has reported its Chosen Mode, which comes with
    /// the rest of its noise control settings. A set sends some of those back
    /// unchanged, so it can't be made before then.
    pub(crate) fn settings_reported(&self) -> Result<(), CommandError> {
        match self.chosen_mode {
            Some(_) => Ok(()),
            None => Err(CommandError::SettingsNotReported),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DeviceState {
    /// Opening the control link and identifying the Model.
    Attaching,
    Attached,
    /// Asked to Power Off and still Connected. Refuses every command, and its
    /// control link closing is part of switching off, not a failure.
    PoweringOff,
    /// Connected, and the app has chosen not to Attach it, so another app or
    /// device can have its control link. It isn't retried.
    Detached,
    /// Connected, but the app can't get a control link.
    NotAttached {
        reason: NotAttachedReason,
    },
}

impl DeviceState {
    /// Whether the Device is a Model the app won't control.
    pub(crate) fn is_unsupported_model(&self) -> bool {
        matches!(
            self,
            DeviceState::NotAttached {
                reason: NotAttachedReason::UnsupportedModel
            }
        )
    }
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
    /// The control link opened, but the Device didn't identify itself in time,
    /// or stopped acknowledging what the app sent.
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
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

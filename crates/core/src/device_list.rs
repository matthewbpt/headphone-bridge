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
    /// `None` until the Device first reports it.
    pub battery: Option<Battery>,
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

/// Battery charge in percent. A part is `None` when its level is unknown,
/// e.g. a bud that isn't connected, or a case with no bud docked in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Battery {
    pub left: Option<u8>,
    pub right: Option<u8>,
    pub case: Option<u8>,
}

//! The Device list snapshot: the core's only output.

use serde::Serialize;

use crate::platform::DeviceAddress;

/// A full snapshot of every Device the core knows about. A new one is emitted
/// on every change.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceList {
    pub devices: Vec<Device>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub address: DeviceAddress,
    pub name: String,
}

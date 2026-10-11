use headphone_bridge_core::platform::{
    ControlLink, DeviceAddress, LinkFailure, Observations, Platform,
};
use tokio::sync::mpsc;
use uuid::Uuid;

/// Stands in for an OS Bluetooth adapter until one exists: it never observes
/// a Device, so nothing can be Connected.
pub struct NoBluetooth;

impl Platform for NoBluetooth {
    fn observations(&self) -> Observations {
        mpsc::unbounded_channel().1
    }

    async fn open_control_link(
        &self,
        _address: &DeviceAddress,
        _service: Uuid,
    ) -> Result<ControlLink, LinkFailure> {
        Err(LinkFailure::DeviceNotConnected)
    }
}

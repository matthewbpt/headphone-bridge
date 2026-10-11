use headphone_bridge_core::platform::{
    ControlLink, DeviceAddress, LinkFailure, Observations, Platform,
};
use tokio::sync::mpsc;
use uuid::Uuid;

/// The platform adapter for this OS.
#[cfg(target_os = "macos")]
pub fn start() -> headphone_bridge_platform_macos::MacBluetooth {
    headphone_bridge_platform_macos::MacBluetooth::start()
}

#[cfg(not(target_os = "macos"))]
pub fn start() -> NoBluetooth {
    NoBluetooth
}

/// Stands in for an OS Bluetooth adapter where none exists yet: it never
/// observes a Device, so nothing can be Connected.
#[cfg_attr(target_os = "macos", allow(dead_code))]
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

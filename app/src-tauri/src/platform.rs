use headphone_bridge_core::platform::{
    ControlLink, DeviceAddress, LinkFailure, Observations, Platform,
};
use tokio::sync::mpsc;
use uuid::Uuid;

/// The platform adapter for this OS.
#[cfg(target_os = "macos")]
pub fn start() -> anyhow::Result<headphone_bridge_platform_macos::MacBluetooth> {
    use anyhow::Context;
    headphone_bridge_platform_macos::MacBluetooth::start().context("couldn't start Bluetooth")
}

#[cfg(target_os = "linux")]
pub fn start() -> anyhow::Result<headphone_bridge_platform_linux::LinuxBluetooth> {
    Ok(headphone_bridge_platform_linux::LinuxBluetooth::start())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn start() -> anyhow::Result<NoBluetooth> {
    Ok(NoBluetooth)
}

/// Stands in for an OS Bluetooth adapter where none exists yet: it never
/// observes a Device, so nothing can be Connected.
#[cfg_attr(
    any(target_os = "macos", target_os = "linux"),
    expect(dead_code, reason = "these have a real adapter")
)]
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

use headphone_bridge_core::platform::{
    ControlLink, DeviceAddress, LinkFailure, Observations, Platform,
};
use uuid::Uuid;

/// The platform adapter for this OS, with the observations it reports.
#[cfg(target_os = "macos")]
pub fn start() -> anyhow::Result<(headphone_bridge_platform_macos::MacBluetooth, Observations)> {
    use anyhow::Context;
    headphone_bridge_platform_macos::MacBluetooth::start().context("couldn't start Bluetooth")
}

#[cfg(target_os = "linux")]
pub fn start() -> anyhow::Result<(
    headphone_bridge_platform_linux::LinuxBluetooth,
    Observations,
)> {
    Ok(headphone_bridge_platform_linux::LinuxBluetooth::start())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn start() -> anyhow::Result<(NoBluetooth, Observations)> {
    // Its sender is dropped at once, so the stream has already ended.
    Ok((NoBluetooth, tokio::sync::mpsc::unbounded_channel().1))
}

/// Stands in for an OS Bluetooth adapter where none exists yet: it never
/// observes a Device, so nothing can be Connected.
#[cfg_attr(
    all(
        any(target_os = "macos", target_os = "linux"),
        not(all(test, debug_assertions))
    ),
    expect(dead_code, reason = "these have a real adapter, and test it only here")
)]
pub struct NoBluetooth;

impl Platform for NoBluetooth {
    async fn open_control_link(
        &self,
        _address: DeviceAddress,
        _service: Uuid,
    ) -> Result<ControlLink, LinkFailure> {
        Err(LinkFailure::DeviceNotConnected)
    }
}

#[cfg(all(test, debug_assertions))]
mod tests {
    use std::time::Duration;

    use headphone_bridge_core::{Core, DeviceState, Model};

    use super::*;

    #[tokio::test]
    async fn the_simulated_device_attaches_over_the_stand_in_for_no_bluetooth() {
        let (_, closed) = tokio::sync::mpsc::unbounded_channel();
        let (platform, observations, simulator) =
            headphone_bridge_simulated::wrap(NoBluetooth, closed);
        let core = Core::builder().start(platform, observations);
        simulator.simulate(Some(Model::Wh1000Xm5)).unwrap();

        let mut list = core.subscribe();
        let attached = list.wait_for(|list| {
            list.devices
                .iter()
                .any(|device| device.state == DeviceState::Attached)
        });
        tokio::time::timeout(Duration::from_secs(5), attached)
            .await
            .expect("the Simulated Device didn't Attach")
            .unwrap();
    }
}

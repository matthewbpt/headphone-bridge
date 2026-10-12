mod support;

use std::time::Duration;

use headphone_bridge_core::platform::Observation;
use headphone_bridge_core::{Command, Core, DeviceState};
use support::{fake_platform, fixture, replay, sony_frame, wait_for, xm5, DeviceLink, FakeOs};

/// Starts a core with a WH-1000XM5 Attached through the whole attach fixture.
async fn attached_xm5() -> (Core, FakeOs, DeviceLink) {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    let mut link = os.next_link().await;
    replay(&mut link, &fixture("wh-1000xm5/attach")).await;
    wait_for(&core, |list| list.devices[0].ambient_level.is_some()).await;
    (core, os, link)
}

#[tokio::test]
async fn power_off_sends_power_set_status_and_the_device_is_powering_off() {
    let (core, _os, mut link) = attached_xm5().await;

    core.command(Command::PowerOff(xm5().address))
        .await
        .unwrap();

    // POWER_SET_STATUS: power off, on, as libmdr and Gadgetbridge send it.
    assert_eq!(written(&mut link).await, sony_frame(1, &[0x24, 0x03, 0x01]));
    wait_for(&core, |list| list.devices[0].state == DeviceState::PoweringOff).await;
}

/// The next frame the core writes.
async fn written(link: &mut DeviceLink) -> Vec<u8> {
    tokio::time::timeout(Duration::from_secs(5), link.written.recv())
        .await
        .expect("timed out waiting for the core to write")
        .expect("link closed")
}

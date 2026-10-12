mod support;

use std::time::Duration;

use headphone_bridge_core::platform::{DeviceAddress, DeviceIdentity, Observation};
use headphone_bridge_core::{Command, CommandError, Core, DeviceState, NotAttachedReason};
use support::{
    buds4pro, fake_platform, fixture, received, replay, sony_frame, wait_for, xm5, DeviceLink,
    FakeOs, SAMSUNG_SERVICE,
};

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

#[tokio::test]
async fn a_buds4pro_refuses_power_off_as_a_missing_capability_and_is_sent_nothing() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    let mut link = os.next_link().await;
    link.written.recv().await.expect("MANAGER_INFO");
    link.send
        .send(received(&fixture("galaxy-buds4-pro/capture-3"))[0].clone())
        .unwrap();
    wait_for(&core, |list| list.devices[0].state == DeviceState::Attached).await;

    assert_eq!(
        core.command(Command::PowerOff(buds4pro().address)).await,
        Err(CommandError::MissingCapability)
    );
    assert_eq!(core.device_list().devices[0].state, DeviceState::Attached);
    assert!(link.written.try_recv().is_err(), "nothing was written");
}

#[tokio::test]
async fn an_xm5_that_is_not_attached_refuses_power_off() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    // The link is open, but the Device hasn't identified itself.
    let _link = os.next_link().await;
    wait_for(&core, |list| list.devices[0].state == DeviceState::Attaching).await;

    assert_eq!(
        core.command(Command::PowerOff(xm5().address)).await,
        Err(CommandError::NotAttached)
    );
}

#[tokio::test]
async fn an_unsupported_model_refuses_power_off() {
    let (platform, os) = fake_platform();
    let core = Core::start(platform);
    let buds2 = DeviceIdentity {
        address: DeviceAddress("A1:B2:C3:00:00:02".into()),
        name: "Galaxy Buds2 (0002)".into(),
        service_uuids: vec![SAMSUNG_SERVICE],
    };
    os.observe(Observation::DeviceConnected(buds2.clone()));
    let unsupported = DeviceState::NotAttached {
        reason: NotAttachedReason::UnsupportedModel,
    };
    wait_for(&core, |list| {
        list.devices.first().map(|device| &device.state) == Some(&unsupported)
    })
    .await;

    assert_eq!(
        core.command(Command::PowerOff(buds2.address)).await,
        Err(CommandError::UnsupportedModel)
    );
}

/// The next frame the core writes.
async fn written(link: &mut DeviceLink) -> Vec<u8> {
    tokio::time::timeout(Duration::from_secs(5), link.written.recv())
        .await
        .expect("timed out waiting for the core to write")
        .expect("link closed")
}

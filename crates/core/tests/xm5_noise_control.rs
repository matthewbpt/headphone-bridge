mod support;

use std::time::Duration;

use headphone_bridge_core::platform::Observation;
use headphone_bridge_core::{Battery, Command, Core, NoiseControlMode};
use support::{fake_platform, fixture, replay, sony_frame, wait_for, xm5, DeviceLink, FakeOs};

/// Starts a core with a WH-1000XM5 Attached through the whole attach
/// fixture, which reports Ambient, Focus on Voice on and Ambient Level 10.
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
async fn attaching_reads_the_chosen_mode_and_ambient_level() {
    let (core, _os, _link) = attached_xm5().await;

    let device = &core.device_list().devices[0];
    assert_eq!(device.chosen_mode, Some(NoiseControlMode::Ambient));
    assert_eq!(device.ambient_level, Some(10));
}

#[tokio::test]
async fn setting_a_mode_resends_focus_on_voice_and_level_and_waits_for_the_device() {
    let (core, _os, mut link) = attached_xm5().await;

    core.command(Command::SetNoiseControlMode(
        xm5().address,
        NoiseControlMode::NoiseCancelling,
    ))
    .await
    .unwrap();

    // NCASM_SET_PARAM: changed, effect on, mode NC, Focus on Voice on as
    // reported, level 10 as reported.
    assert_eq!(
        written(&mut link).await,
        sony_frame(1, &[0x68, 0x17, 0x01, 0x01, 0x00, 0x01, 0x0a])
    );
    // The ACK is only a receipt. A battery notification after it shows it
    // has been handled.
    link.send.send(ACK_SEQ_0.to_vec()).unwrap();
    link.send.send(sony_frame(0, &[0x25, 0x00, 68, 0x00])).unwrap();
    let list = wait_for(&core, |list| {
        list.devices[0].battery == Some(Battery::Headphones { level: 68 })
    })
    .await;
    assert_eq!(list.devices[0].chosen_mode, Some(NoiseControlMode::Ambient));

    link.send
        .send(sony_frame(1, &[0x69, 0x17, 0x01, 0x01, 0x00, 0x01, 0x0a]))
        .unwrap();
    wait_for(&core, |list| {
        list.devices[0].chosen_mode == Some(NoiseControlMode::NoiseCancelling)
    })
    .await;
}

/// The next frame the core writes.
async fn written(link: &mut DeviceLink) -> Vec<u8> {
    tokio::time::timeout(Duration::from_secs(5), link.written.recv())
        .await
        .expect("timed out waiting for the core to write")
        .expect("link closed")
}

/// An ACK with sequence number 0: for a frame sent with 1.
const ACK_SEQ_0: [u8; 9] = [0x3e, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x3c];

mod support;

use headphone_bridge_core::platform::Observation;
use headphone_bridge_core::{Battery, Command, CommandError, Core, DeviceState, LevelRange};
use support::{
    buds4pro, fake_platform, fixture, received, samsung_frame, wait_for, DeviceLink, FakeOs,
};

/// Starts a core with a Buds4 Pro Attached through capture-3's first extended
/// status, which reports Ambient Level 2 (byte 23).
async fn attached_buds4pro() -> (Core, FakeOs, DeviceLink) {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    let mut link = os.next_link().await;
    link.written.recv().await.expect("MANAGER_INFO");
    link.send
        .send(received(&fixture("galaxy-buds4-pro/capture-3"))[0].clone())
        .unwrap();
    wait_for(&core, |list| list.devices[0].state == DeviceState::Attached).await;
    (core, os, link)
}

#[tokio::test]
async fn the_snapshot_carries_the_models_ambient_level_range_and_the_reported_level() {
    let (core, _os, _link) = attached_buds4pro().await;

    let list = wait_for(&core, |list| list.devices[0].ambient_level == Some(2)).await;
    assert_eq!(
        list.devices[0].capabilities.ambient_level,
        Some(LevelRange { min: 0, max: 4 })
    );
}

/// `AMBIENT_VOLUME` (0x84) with level 4.
const SET_AMBIENT_LEVEL_4: [u8; 8] = [0xfd, 0x04, 0x00, 0x84, 0x04, 0xd8, 0x97, 0xdd];
const MANAGER_INFO: [u8; 10] = [0xfd, 0x06, 0x00, 0x88, 0x01, 0x01, 0x22, 0xda, 0x58, 0xdd];

/// capture-3's first extended status with byte 23 (Ambient Level) set to `level`.
fn extended_status_with_ambient_level(level: u8) -> Vec<u8> {
    let mut status = received(&fixture("galaxy-buds4-pro/capture-3"))[0][4..66].to_vec();
    status[23] = level;
    samsung_frame(0x61, &status)
}

#[tokio::test]
async fn setting_a_level_sends_ambient_volume_and_changes_nothing_until_the_device_confirms() {
    let (core, _os, mut link) = attached_buds4pro().await;
    wait_for(&core, |list| list.devices[0].ambient_level == Some(2)).await;

    core.command(Command::SetAmbientLevel(buds4pro().address, 4))
        .await
        .unwrap();

    assert_eq!(link.written.recv().await.unwrap(), SET_AMBIENT_LEVEL_4);
    // Like a mode set, polled with MANAGER_INFO for a fresh extended status.
    assert_eq!(link.written.recv().await.unwrap(), MANAGER_INFO);
    // A status that changes the battery shows the set has been handled.
    link.send
        .send(samsung_frame(
            0x60,
            &[0x01, 0x50, 0x50, 0x01, 0x01, 0x11, 0x00, 0x00],
        ))
        .unwrap();
    let list = wait_for(&core, |list| {
        matches!(
            list.devices[0].battery,
            Some(Battery::Buds { left: Some(80), .. })
        )
    })
    .await;
    assert_eq!(list.devices[0].ambient_level, Some(2));

    link.send
        .send(extended_status_with_ambient_level(4))
        .unwrap();
    wait_for(&core, |list| list.devices[0].ambient_level == Some(4)).await;
}

#[tokio::test]
async fn a_level_outside_the_models_range_is_rejected_before_anything_is_sent() {
    let (core, _os, mut link) = attached_buds4pro().await;

    let result = core
        .command(Command::SetAmbientLevel(buds4pro().address, 5))
        .await;

    assert_eq!(result, Err(CommandError::OutOfRange));
    drop(core);
    assert_eq!(link.written.recv().await, None, "nothing else written");
}

#[tokio::test]
async fn a_level_is_not_sent_before_the_model_and_its_capabilities_are_known() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    let mut link = os.next_link().await;
    link.written.recv().await.expect("MANAGER_INFO");

    let result = core
        .command(Command::SetAmbientLevel(buds4pro().address, 1))
        .await;

    assert_eq!(result, Err(CommandError::NotAttached));
    drop(core);
    assert_eq!(link.written.recv().await, None, "nothing else written");
}

#[tokio::test]
async fn an_ack_confirms_the_level_only_when_it_carries_the_one_asked_for() {
    let (core, _os, mut link) = attached_buds4pro().await;
    wait_for(&core, |list| list.devices[0].ambient_level == Some(2)).await;

    core.command(Command::SetAmbientLevel(buds4pro().address, 0))
        .await
        .unwrap();
    assert_eq!(
        link.written.recv().await.unwrap(),
        [0xfd, 0x04, 0x00, 0x84, 0x00, 0x5c, 0xd7, 0xdd]
    );
    // A Buds4 Pro ACKs AMBIENT_VOLUME with the applied level (frame log,
    // 2026-10-11). An ACK with any other value, such as one for an earlier
    // set, is only a receipt.
    link.send.send(samsung_frame(0x42, &[0x84, 0x03])).unwrap();
    link.send
        .send(samsung_frame(
            0x60,
            &[0x01, 0x50, 0x50, 0x01, 0x01, 0x11, 0x00, 0x00],
        ))
        .unwrap();
    let list = wait_for(&core, |list| {
        matches!(
            list.devices[0].battery,
            Some(Battery::Buds { left: Some(80), .. })
        )
    })
    .await;
    assert_eq!(list.devices[0].ambient_level, Some(2));

    link.send.send(samsung_frame(0x42, &[0x84, 0x00])).unwrap();

    wait_for(&core, |list| list.devices[0].ambient_level == Some(0)).await;
}

#[tokio::test]
async fn a_level_changed_from_the_phone_appears_through_the_extended_status() {
    let (core, _os, link) = attached_buds4pro().await;
    wait_for(&core, |list| list.devices[0].ambient_level == Some(2)).await;

    link.send
        .send(extended_status_with_ambient_level(0))
        .unwrap();

    wait_for(&core, |list| list.devices[0].ambient_level == Some(0)).await;
}

#[tokio::test]
async fn a_level_the_device_announces_appears_live() {
    let (core, _os, link) = attached_buds4pro().await;
    wait_for(&core, |list| list.devices[0].ambient_level == Some(2)).await;

    // AMBIENT_VOLUME (132) from the Device carries the level in byte 0.
    link.send.send(samsung_frame(0x84, &[0x03])).unwrap();

    wait_for(&core, |list| list.devices[0].ambient_level == Some(3)).await;
}

#[tokio::test]
async fn a_level_outside_the_models_range_from_the_device_is_ignored() {
    let (core, _os, link) = attached_buds4pro().await;
    wait_for(&core, |list| list.devices[0].ambient_level == Some(2)).await;

    link.send.send(samsung_frame(0x84, &[0x09])).unwrap();
    link.send
        .send(extended_status_with_ambient_level(0xff))
        .unwrap();
    link.send
        .send(samsung_frame(
            0x60,
            &[0x01, 0x50, 0x50, 0x01, 0x01, 0x11, 0x00, 0x00],
        ))
        .unwrap();

    let list = wait_for(&core, |list| {
        matches!(
            list.devices[0].battery,
            Some(Battery::Buds { left: Some(80), .. })
        )
    })
    .await;
    assert_eq!(list.devices[0].ambient_level, Some(2));
}

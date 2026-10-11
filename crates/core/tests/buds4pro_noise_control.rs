mod support;

use headphone_bridge_core::platform::{DeviceAddress, DeviceIdentity, Observation};
use headphone_bridge_core::{Command, CommandError, Core, DeviceState, NoiseControlMode};
use support::{
    buds4pro, fake_platform, fixture, received, samsung_frame, wait_for, DeviceLink, FakeOs,
    SAMSUNG_SERVICE,
};

/// `NOISE_CONTROLS` (0x78) with Noise Cancelling (1).
const SET_NOISE_CANCELLING: [u8; 8] = [0xfd, 0x04, 0x00, 0x78, 0x01, 0xd1, 0x91, 0xdd];
const MANAGER_INFO: [u8; 10] = [0xfd, 0x06, 0x00, 0x88, 0x01, 0x01, 0x22, 0xda, 0x58, 0xdd];

/// Starts a core with a Buds4 Pro Attached through capture-3's first extended
/// status, which reports Ambient as the Chosen Mode.
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
async fn the_extended_status_reports_the_chosen_mode() {
    let (core, _os, _link) = attached_buds4pro().await;

    wait_for(&core, |list| {
        list.devices[0].chosen_mode == Some(NoiseControlMode::Ambient)
    })
    .await;
}

#[tokio::test]
async fn setting_a_mode_sends_noise_controls_then_polls_and_the_ack_alone_changes_nothing() {
    let (core, _os, mut link) = attached_buds4pro().await;

    core.command(Command::SetNoiseControlMode(
        buds4pro().address,
        NoiseControlMode::NoiseCancelling,
    ))
    .await
    .unwrap();

    assert_eq!(link.written.recv().await.unwrap(), SET_NOISE_CANCELLING);
    // capture-2: after a set, the Device stayed silent until the next
    // MANAGER_INFO, then confirmed within half a second.
    assert_eq!(link.written.recv().await.unwrap(), MANAGER_INFO);
    // A Buds4 Pro ACKs with a result code (0), not the mode: only a receipt.
    link.send.send(samsung_frame(0x42, &[0x78, 0x00])).unwrap();
    // A status that changes the battery shows the ACK has been handled.
    link.send
        .send(samsung_frame(
            0x60,
            &[0x01, 0x50, 0x50, 0x01, 0x01, 0x11, 0x00, 0x00],
        ))
        .unwrap();
    let list = wait_for(&core, |list| {
        list.devices[0].battery.is_some_and(|b| b.left == Some(80))
    })
    .await;
    assert_eq!(list.devices[0].chosen_mode, Some(NoiseControlMode::Ambient));
}

#[tokio::test]
async fn a_mode_set_from_the_app_appears_once_the_noise_controls_update_confirms_it() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    let mut link = os.next_link().await;
    link.written.recv().await.expect("MANAGER_INFO");
    // capture-2: the extended status after the handshake reports Off. After
    // the app sets Ambient, the Device confirms with `119: 02 12 01 00 09 09
    // 02 02 02 02 02`, whose cause byte (6) is not the gesture flag.
    let frames = received(&fixture("galaxy-buds4-pro/capture-2"));
    link.send.send(frames[0].clone()).unwrap();
    wait_for(&core, |list| {
        list.devices[0].chosen_mode == Some(NoiseControlMode::Off)
    })
    .await;

    core.command(Command::SetNoiseControlMode(
        buds4pro().address,
        NoiseControlMode::Ambient,
    ))
    .await
    .unwrap();
    assert_eq!(
        link.written.recv().await.unwrap(),
        [0xfd, 0x04, 0x00, 0x78, 0x02, 0xb2, 0xa1, 0xdd]
    );
    link.send.send(frames.last().unwrap().clone()).unwrap();

    wait_for(&core, |list| {
        list.devices[0].chosen_mode == Some(NoiseControlMode::Ambient)
    })
    .await;
}

#[tokio::test]
async fn a_placement_change_alone_does_not_change_the_chosen_mode() {
    let (core, _os, link) = attached_buds4pro().await;

    // capture-2: both buds out of the ear, and the Device pauses to Off.
    link.send
        .send(samsung_frame(
            0x77,
            &[
                0x00, 0x22, 0x01, 0x00, 0x09, 0x09, 0x02, 0x00, 0x00, 0x02, 0x02,
            ],
        ))
        .unwrap();
    link.send
        .send(samsung_frame(
            0x60,
            &[0x01, 0x50, 0x50, 0x01, 0x01, 0x22, 0x00, 0x00],
        ))
        .unwrap();

    let list = wait_for(&core, |list| {
        list.devices[0].battery.is_some_and(|b| b.left == Some(80))
    })
    .await;
    assert_eq!(list.devices[0].chosen_mode, Some(NoiseControlMode::Ambient));
}

#[tokio::test]
async fn a_gesture_on_the_device_changes_the_chosen_mode_live() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    let link = os.next_link().await;
    // capture-4: the extended status reports Ambient, then pinch-and-hold
    // gestures alternate Noise Cancelling and Ambient (`119: 0m 11 01 00 09 09
    // 01 …`, cause byte 6 = 1), ending on Noise Cancelling.
    let frames = received(&fixture("galaxy-buds4-pro/capture-4"));
    link.send.send(frames[0].clone()).unwrap();
    wait_for(&core, |list| {
        list.devices[0].chosen_mode == Some(NoiseControlMode::Ambient)
    })
    .await;

    for frame in &frames[1..] {
        link.send.send(frame.clone()).unwrap();
    }

    wait_for(&core, |list| {
        list.devices[0].chosen_mode == Some(NoiseControlMode::NoiseCancelling)
    })
    .await;
}

#[tokio::test]
async fn a_mode_changed_from_the_phone_appears_through_the_extended_status() {
    let (core, _os, link) = attached_buds4pro().await;

    // capture-3's first extended status with byte 12 (mode) set to Adaptive.
    let mut status = received(&fixture("galaxy-buds4-pro/capture-3"))[0][4..66].to_vec();
    status[12] = 0x03;
    link.send.send(samsung_frame(0x61, &status)).unwrap();

    wait_for(&core, |list| {
        list.devices[0].chosen_mode == Some(NoiseControlMode::Adaptive)
    })
    .await;
}

#[tokio::test]
async fn a_mode_is_not_sent_before_the_model_and_its_capabilities_are_known() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    let mut link = os.next_link().await;
    link.written.recv().await.expect("MANAGER_INFO");

    let result = core
        .command(Command::SetNoiseControlMode(
            buds4pro().address,
            NoiseControlMode::NoiseCancelling,
        ))
        .await;

    assert_eq!(result, Err(CommandError::NotAttached));
    drop(core);
    assert_eq!(link.written.recv().await, None, "nothing else written");
}

#[tokio::test]
async fn a_mode_is_never_sent_to_an_unsupported_model() {
    let (platform, os) = fake_platform();
    let core = Core::start(platform);
    let buds3pro = DeviceIdentity {
        address: DeviceAddress("10:20:30:40:50:60".into()),
        name: "Galaxy Buds3 Pro (9F3C)".into(),
        service_uuids: vec![SAMSUNG_SERVICE],
    };
    os.observe(Observation::DeviceConnected(buds3pro.clone()));
    wait_for(&core, |list| list.devices.len() == 1).await;

    let result = core
        .command(Command::SetNoiseControlMode(
            buds3pro.address,
            NoiseControlMode::Off,
        ))
        .await;

    assert_eq!(result, Err(CommandError::NotAttached));
}

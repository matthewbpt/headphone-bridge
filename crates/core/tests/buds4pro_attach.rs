mod support;

use headphone_bridge_core::platform::{DeviceAddress, DeviceIdentity, LinkFailure, Observation};
use headphone_bridge_core::{
    Battery, Capabilities, Core, DeviceState, LevelRange, Model, NoiseControlMode,
    NotAttachedReason,
};
use support::{
    buds4pro, fake_platform, fixture, received, samsung_frame, wait_for, SAMSUNG_SERVICE,
};

const MANAGER_INFO: [u8; 10] = [0xfd, 0x06, 0x00, 0x88, 0x01, 0x01, 0x22, 0xda, 0x58, 0xdd];

#[tokio::test]
async fn a_connected_buds4_pro_is_attached_with_the_manager_info_handshake() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);

    os.observe(Observation::DeviceConnected(buds4pro()));
    let mut link = os.next_link().await;

    assert_eq!(link.address, buds4pro().address);
    assert_eq!(link.service, SAMSUNG_SERVICE);
    assert_eq!(link.written.recv().await.unwrap(), MANAGER_INFO);
    let list = core.device_list();
    assert_eq!(list.devices.len(), 1);
    assert_eq!(list.devices[0].name, "Galaxy Buds4 Pro (A1B2)");
    assert_eq!(list.devices[0].state, DeviceState::Attaching);
}

#[tokio::test]
async fn the_extended_status_identifies_the_buds4_pro_and_its_battery() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    let link = os.next_link().await;

    // capture-3: the 97 after the handshake has L 100 %, R 97 %, R in the case
    // (so the case level is live) at 100 %.
    let frames = received(&fixture("galaxy-buds4-pro/capture-3"));
    link.send.send(frames[0].clone()).unwrap();

    let list = wait_for(&core, |list| list.devices[0].state == DeviceState::Attached).await;
    let device = &list.devices[0];
    assert_eq!(device.model, Some(Model::GalaxyBuds4Pro));
    assert_eq!(
        device.capabilities,
        Capabilities {
            noise_control_modes: vec![
                NoiseControlMode::Off,
                NoiseControlMode::NoiseCancelling,
                NoiseControlMode::Ambient,
                NoiseControlMode::Adaptive,
            ],
            ambient_level: Some(LevelRange { min: 0, max: 4 }),
            noise_cancelling_level: Some(LevelRange { min: 0, max: 4 }),
            power_off: false,
            // Pinned in buds4pro_equalizer.
            equalizer: Model::GalaxyBuds4Pro.capabilities().equalizer,
        }
    );
    assert_eq!(
        device.battery,
        Some(Battery::Buds {
            left: Some(100),
            right: Some(97),
            case: Some(100)
        })
    );
}

#[tokio::test]
async fn battery_follows_status_updates() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    let link = os.next_link().await;

    // capture-3 ends with `96: 01 64 62 01 01 11 64 00`: L 100 %, R 98 %, both
    // in the ear, so the case byte isn't live and the case level is unknown.
    for frame in received(&fixture("galaxy-buds4-pro/capture-3")) {
        link.send.send(frame).unwrap();
    }

    wait_for(&core, |list| {
        list.devices[0].battery
            == Some(Battery::Buds {
                left: Some(100),
                right: Some(98),
                case: None,
            })
    })
    .await;
}

#[tokio::test]
async fn a_charging_bud_counts_as_docked_before_its_placement_says_so() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    let link = os.next_link().await;

    // capture-1c, as the right bud drops into the case: `96: 01 64 62 01 01 22
    // 64 04` still places both buds out of the ear (0x22), but the right bud's
    // charging bit (0x04) is set, so the case level is live.
    let frames = received(&fixture("galaxy-buds4-pro/capture-1c"));
    let docking = frames
        .iter()
        .position(|frame| {
            frame[3] == 0x60 && frame[4..12] == [0x01, 0x64, 0x62, 0x01, 0x01, 0x22, 0x64, 0x04]
        })
        .unwrap();
    for frame in &frames[..=docking] {
        link.send.send(frame.clone()).unwrap();
    }

    wait_for(&core, |list| {
        list.devices[0].battery
            == Some(Battery::Buds {
                left: Some(100),
                right: Some(98),
                case: Some(100),
            })
    })
    .await;
}

#[tokio::test]
async fn the_case_level_is_only_read_while_a_bud_is_docked() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    let link = os.next_link().await;
    link.send
        .send(received(&fixture("galaxy-buds4-pro/capture-3"))[0].clone())
        .unwrap();

    // Both buds out: the case byte (0) is stale. Then the right bud docks, in a
    // status whose case byte (255) is invalid: the last live reading stands.
    link.send
        .send(samsung_frame(
            0x60,
            &[0x01, 0x64, 0x61, 0x01, 0x01, 0x11, 0x00, 0x00],
        ))
        .unwrap();
    wait_for(&core, |list| {
        matches!(
            list.devices[0].battery,
            Some(Battery::Buds { case: None, .. })
        )
    })
    .await;
    link.send
        .send(samsung_frame(
            0x60,
            &[0x01, 0x64, 0x61, 0x01, 0x01, 0x13, 0xff, 0x04],
        ))
        .unwrap();

    wait_for(&core, |list| {
        list.devices[0].battery
            == Some(Battery::Buds {
                left: Some(100),
                right: Some(97),
                case: Some(100),
            })
    })
    .await;
}

#[tokio::test]
async fn a_seven_byte_status_update_without_the_charging_byte_updates_battery() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    let link = os.next_link().await;
    link.send
        .send(received(&fixture("galaxy-buds4-pro/capture-3"))[0].clone())
        .unwrap();

    // L 80 %, R 70 %, both in the ear (0x11), so the case byte isn't live.
    link.send
        .send(samsung_frame(
            0x60,
            &[0x01, 0x50, 0x46, 0x01, 0x01, 0x11, 0x40],
        ))
        .unwrap();

    wait_for(&core, |list| {
        list.devices[0].battery
            == Some(Battery::Buds {
                left: Some(80),
                right: Some(70),
                case: None,
            })
    })
    .await;
}

#[tokio::test]
async fn a_device_without_a_supported_vendor_service_is_ignored() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);

    os.observe(Observation::DeviceConnected(DeviceIdentity {
        address: DeviceAddress("11:22:33:44:55:66".into()),
        name: "Magic Keyboard".into(),
        service_uuids: vec![],
    }));
    os.observe(Observation::DeviceConnected(buds4pro()));

    let link = os.next_link().await;
    assert_eq!(link.address, buds4pro().address);
    assert_eq!(core.device_list().devices.len(), 1);
}

#[tokio::test]
async fn a_samsung_device_whose_name_is_not_on_the_allowlist_is_never_attached() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    let buds3pro = DeviceIdentity {
        address: DeviceAddress("10:20:30:40:50:60".into()),
        name: "Galaxy Buds3 Pro (9F3C)".into(),
        service_uuids: vec![SAMSUNG_SERVICE],
    };

    os.observe(Observation::DeviceConnected(buds3pro));
    // A later Device that is Attached proves the first was handled without a link.
    os.observe(Observation::DeviceConnected(buds4pro()));

    assert_eq!(os.next_link().await.address, buds4pro().address);
    let list = wait_for(&core, |list| list.devices.len() == 2).await;
    let device = list
        .devices
        .iter()
        .find(|d| d.name.starts_with("Galaxy Buds3"))
        .unwrap();
    assert_eq!(device.model, None);
    assert_eq!(
        device.state,
        DeviceState::NotAttached {
            reason: NotAttachedReason::UnsupportedModel
        }
    );
}

#[tokio::test]
async fn a_device_whose_extended_status_names_another_model_is_detached_after_the_handshake() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    let mut link = os.next_link().await;

    // capture-3's first 97 with its device ids (bytes 14–17) changed to 340, a
    // Galaxy Buds3 Pro.
    link.send
        .send(samsung_frame(0x61, &BUDS3_PRO_EXTENDED_STATUS))
        .unwrap();

    let list = wait_for(&core, |list| {
        list.devices[0].state != DeviceState::Attaching
    })
    .await;
    assert_eq!(
        list.devices[0].state,
        DeviceState::NotAttached {
            reason: NotAttachedReason::UnsupportedModel
        }
    );
    assert_eq!(list.devices[0].model, None);
    assert_eq!(list.devices[0].battery, None);
    assert_eq!(link.written.recv().await.unwrap(), MANAGER_INFO);
    assert_eq!(
        link.written.recv().await,
        None,
        "the link must close with nothing else written"
    );
}

#[tokio::test]
async fn a_link_that_cannot_be_opened_leaves_the_device_connected_with_the_reason() {
    for (failure, reason) in [
        (
            LinkFailure::InUseByAnotherProcess,
            NotAttachedReason::InUseByAnotherProcess,
        ),
        (
            LinkFailure::BluetoothPermissionDenied,
            NotAttachedReason::BluetoothPermissionDenied,
        ),
        (
            LinkFailure::ServiceNotFound,
            NotAttachedReason::ServiceNotFound,
        ),
        (
            LinkFailure::DeviceNotConnected,
            NotAttachedReason::DeviceNotConnected,
        ),
        (
            LinkFailure::Other("timed out".into()),
            NotAttachedReason::Other("timed out".into()),
        ),
    ] {
        let (platform, os) = fake_platform();
        let core = Core::start(platform);
        os.fail_links(&buds4pro().address, failure);

        os.observe(Observation::DeviceConnected(buds4pro()));

        let list = wait_for(&core, |list| {
            list.devices
                .first()
                .is_some_and(|d| d.state != DeviceState::Attaching)
        })
        .await;
        assert_eq!(list.devices[0].state, DeviceState::NotAttached { reason });
    }
}

#[tokio::test]
async fn a_dropped_link_detaches_the_device() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    let link = os.next_link().await;
    link.send
        .send(received(&fixture("galaxy-buds4-pro/capture-3"))[0].clone())
        .unwrap();
    wait_for(&core, |list| list.devices[0].state == DeviceState::Attached).await;

    drop(link);

    let list = wait_for(&core, |list| list.devices[0].state != DeviceState::Attached).await;
    let device = &list.devices[0];
    assert_eq!(
        device.state,
        DeviceState::NotAttached {
            reason: NotAttachedReason::LinkDropped
        }
    );
    assert_eq!(device.battery, None, "no stale values remain");
    assert_eq!(device.chosen_mode, None, "no stale values remain");
    // It's the same headphones while they stay Connected.
    assert_eq!(device.model, Some(Model::GalaxyBuds4Pro));
    assert_eq!(device.capabilities, Model::GalaxyBuds4Pro.capabilities());
}

#[tokio::test]
async fn a_disconnected_device_is_removed_and_its_link_closed() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    let mut link = os.next_link().await;
    link.send
        .send(received(&fixture("galaxy-buds4-pro/capture-3"))[0].clone())
        .unwrap();
    wait_for(&core, |list| list.devices[0].state == DeviceState::Attached).await;

    os.observe(Observation::DeviceDisconnected(buds4pro().address));

    wait_for(&core, |list| list.devices.is_empty()).await;
    assert_eq!(link.written.recv().await.unwrap(), MANAGER_INFO);
    assert_eq!(link.written.recv().await, None);
}

#[tokio::test]
async fn frames_are_reassembled_from_any_fragmentation() {
    // raw-1 keeps the wire bytes exactly, with sequence and unknown flag bits
    // in the headers. Its last status is `96: 01 62 5e 01 01 11 64 00`: L 98 %,
    // R 94 %, both in the ear.
    let mut stream = vec![0x00, 0xdd, 0xfd, 0x13]; // garbage, including a fake start byte
    for frame in received(&fixture("galaxy-buds4-pro/raw-1")) {
        stream.extend(frame);
    }
    for chunk_size in [1, 2, 7, 64, stream.len()] {
        let (platform, mut os) = fake_platform();
        let core = Core::start(platform);
        os.observe(Observation::DeviceConnected(buds4pro()));
        let link = os.next_link().await;

        for chunk in stream.chunks(chunk_size) {
            link.send.send(chunk.to_vec()).unwrap();
        }

        let list = wait_for(&core, |list| {
            list.devices[0].battery
                == Some(Battery::Buds {
                    left: Some(98),
                    right: Some(94),
                    case: None,
                })
        })
        .await;
        assert_eq!(
            list.devices[0].state,
            DeviceState::Attached,
            "chunks of {chunk_size}"
        );
    }
}

#[tokio::test]
async fn garbage_that_looks_like_the_start_of_a_long_frame_does_not_hide_a_real_one() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    let link = os.next_link().await;

    // A start byte whose header claims 1023 bytes, more than will ever arrive.
    let mut bytes = vec![0xfd, 0xff, 0x03];
    bytes.extend(&received(&fixture("galaxy-buds4-pro/capture-3"))[0]);
    link.send.send(bytes).unwrap();

    wait_for(&core, |list| list.devices[0].state == DeviceState::Attached).await;
}

#[tokio::test]
async fn a_frame_that_fails_its_crc_is_ignored() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    let link = os.next_link().await;

    // Were the corrupted Buds3 Pro status accepted, the Device would be Detached
    // as unsupported before the real one arrives.
    let mut corrupted = samsung_frame(0x61, &BUDS3_PRO_EXTENDED_STATUS);
    let crc_at = corrupted.len() - 2;
    corrupted[crc_at] ^= 0xff;
    link.send.send(corrupted).unwrap();
    link.send
        .send(received(&fixture("galaxy-buds4-pro/capture-3"))[0].clone())
        .unwrap();

    let list = wait_for(&core, |list| {
        list.devices[0].state != DeviceState::Attaching
    })
    .await;
    assert_eq!(list.devices[0].state, DeviceState::Attached);
}

/// capture-3's first extended status with device ids 340 (Galaxy Buds3 Pro).
const BUDS3_PRO_EXTENDED_STATUS: [u8; 62] = [
    0x04, 0x0d, 0x64, 0x61, 0x01, 0x01, 0x13, 0x64, 0x00, 0x03, 0xff, 0x22, 0x02, 0x00, 0x54, 0x01,
    0x54, 0x01, 0x03, 0x00, 0x04, 0x99, 0x00, 0x02, 0x04, 0x10, 0x01, 0x00, 0x00, 0x00, 0x11, 0x02,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x01, 0x00,
    0x01, 0x01, 0x00, 0x00, 0x00, 0xff, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
];

#![cfg(test)]

mod support;

use std::path::PathBuf;

use headphone_bridge_core::frame_log::FrameRecord;
use headphone_bridge_core::platform::{DeviceAddress, DeviceIdentity, Observation};
use headphone_bridge_core::{
    Battery, Capabilities, Command, Core, DeviceState, LevelRange, Model, NoiseControlMode,
    NotAttachedReason,
};
use support::{
    fake_platform, fixture, received, replay, sent, sony_frame, wait_for, xm5, SONY_SERVICE,
};

/// `CONNECT_GET_PROTOCOL_INFO` with sequence number 0.
const GET_PROTOCOL_INFO: [u8; 11] = [
    0x3e, 0x0c, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x0e, 0x3c,
];

#[tokio::test]
async fn a_connected_xm5_is_attached_on_the_v2_service_by_asking_for_its_protocol_info() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);

    os.observe(Observation::DeviceConnected(xm5()));
    let mut link = os.next_link().await;

    assert_eq!(link.address, xm5().address);
    assert_eq!(link.service, SONY_SERVICE);
    assert_eq!(link.written.recv().await.unwrap(), GET_PROTOCOL_INFO);
    let list = core.device_list();
    assert_eq!(list.devices.len(), 1);
    assert_eq!(list.devices[0].name, "WH-1000XM5");
    assert_eq!(list.devices[0].state, DeviceState::Attaching);
}

#[tokio::test]
async fn the_model_name_identifies_the_xm5_and_its_battery_is_read() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    let mut link = os.next_link().await;

    // Up to and including the reply to the battery GET: 70 %.
    let records = fixture("wh-1000xm5/attach");
    replay(&mut link, &records[..16]).await;

    let list = wait_for(&core, |list| list.devices[0].battery.is_some()).await;
    let device = &list.devices[0];
    assert_eq!(device.state, DeviceState::Attached);
    assert_eq!(device.model, Some(Model::Wh1000Xm5));
    assert_eq!(
        device.capabilities,
        Capabilities {
            noise_control_modes: vec![
                NoiseControlMode::Off,
                NoiseControlMode::NoiseCancelling,
                NoiseControlMode::Ambient,
            ],
            ambient_level: Some(LevelRange { min: 1, max: 20 }),
            noise_cancelling_level: None,
            power_off: true,
            // Its Presets are covered in xm5_equalizer.
            equalizer: device.capabilities.equalizer.clone(),
        }
    );
    assert_eq!(device.battery, Some(Battery::Headphones { level: 70 }));
}

#[tokio::test]
async fn battery_follows_notifications() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    let mut link = os.next_link().await;

    // The fixture ends with a notification of 69 %, which the core ACKs.
    replay(&mut link, &fixture("wh-1000xm5/attach")).await;

    wait_for(&core, |list| {
        list.devices[0].battery == Some(Battery::Headphones { level: 69 })
    })
    .await;
}

#[tokio::test]
async fn a_real_xm5_attach_replays_from_its_frame_log() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    let mut link = os.next_link().await;

    // Firmware 2.5.1, battery 57 %.
    replay(&mut link, &fixture("wh-1000xm5/real-attach")).await;

    let list = wait_for(&core, |list| list.devices[0].battery.is_some()).await;
    let device = &list.devices[0];
    assert_eq!(device.state, DeviceState::Attached);
    assert_eq!(device.model, Some(Model::Wh1000Xm5));
    assert_eq!(device.battery, Some(Battery::Headphones { level: 57 }));
}

#[tokio::test]
async fn another_sony_model_is_detached_having_been_sent_only_reads() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    let wh1000xm6 = DeviceIdentity {
        address: DeviceAddress("AC:80:0A:44:55:66".into()),
        name: "WH-1000XM6".into(),
        service_uuids: vec![SONY_SERVICE],
    };
    os.observe(Observation::DeviceConnected(wh1000xm6));
    let mut link = os.next_link().await;

    // The fixture up to the GET for the model name, then its reply, naming
    // another Model: `05 01 0a "WH-1000XM6"`.
    replay(&mut link, &fixture("wh-1000xm5/attach")[..5]).await;
    link.send
        .send(sony_frame(1, b"\x05\x01\x0aWH-1000XM6"))
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
    assert_eq!(
        link.written.recv().await,
        None,
        "the link must close with nothing else written"
    );
}

#[tokio::test]
async fn a_battery_level_that_needs_escaping_is_read() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    let mut link = os.next_link().await;
    replay(&mut link, &fixture("wh-1000xm5/attach")).await;

    // 60 % is 0x3C, the end byte, so it is escaped as `3d 2c`.
    let notification = sony_frame(1, &[0x25, 0x00, 60, 0x00]);
    assert_eq!(notification[9..11], [0x3d, 0x2c]);
    link.send.send(notification).unwrap();

    wait_for(&core, |list| {
        list.devices[0].battery == Some(Battery::Headphones { level: 60 })
    })
    .await;
}

#[tokio::test]
async fn a_device_that_only_lists_the_battery_with_threshold_is_asked_for_that() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    let mut link = os.next_link().await;
    replay(&mut link, &fixture("wh-1000xm5/attach")[..9]).await;

    // Support functions: battery with threshold (0x28) only.
    link.send.send(ACK_SEQ_1.to_vec()).unwrap();
    link.send
        .send(sony_frame(0, &[0x07, 0x00, 0x01, 0x28, 0xff]))
        .unwrap();
    assert_eq!(link.written.recv().await.unwrap(), ACK_SEQ_1);
    // `22 08`: POWER_GET_STATUS, BATTERY_WITH_THRESHOLD.
    assert_eq!(
        link.written.recv().await.unwrap(),
        [0x3e, 0x0c, 0x01, 0x00, 0x00, 0x00, 0x02, 0x22, 0x08, 0x39, 0x3c]
    );
    link.send.send(ACK_SEQ_0.to_vec()).unwrap();
    // 55 %, not charging, threshold 30 %.
    link.send
        .send(sony_frame(1, &[0x23, 0x08, 55, 0x00, 30]))
        .unwrap();

    wait_for(&core, |list| {
        list.devices[0].battery == Some(Battery::Headphones { level: 55 })
    })
    .await;
}

#[tokio::test]
async fn a_frame_that_fails_its_checksum_is_ignored() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    let mut link = os.next_link().await;
    let records = fixture("wh-1000xm5/attach");
    replay(&mut link, &records[..6]).await;

    // Were the corrupted WH-1000XM6 model name accepted, the Device would be
    // Detached as unsupported before the real one arrives.
    let mut corrupted = sony_frame(1, b"\x05\x01\x0aWH-1000XM6");
    let checksum_at = corrupted.len() - 2;
    corrupted[checksum_at] ^= 0x01;
    link.send.send(corrupted).unwrap();
    replay(&mut link, &records[6..]).await;

    let list = wait_for(&core, |list| list.devices[0].battery.is_some()).await;
    assert_eq!(list.devices[0].state, DeviceState::Attached);
}

#[tokio::test]
async fn frames_are_reassembled_from_any_fragmentation() {
    let records = fixture("wh-1000xm5/attach");
    // Garbage, including a stray start byte, before the Device's frames.
    let mut stream = vec![0x00, 0x3c, 0x3e, 0x0c, 0x3c, 0x3e];
    for frame in received(&records) {
        stream.extend(frame);
    }
    for chunk_size in [1, 2, 7, 64, stream.len()] {
        let (platform, mut os) = fake_platform();
        let core = Core::start(platform);
        os.observe(Observation::DeviceConnected(xm5()));
        let mut link = os.next_link().await;

        for chunk in stream.chunks(chunk_size) {
            link.send.send(chunk.to_vec()).unwrap();
        }

        let list = wait_for(&core, |list| {
            list.devices[0].battery == Some(Battery::Headphones { level: 69 })
        })
        .await;
        assert_eq!(
            list.devices[0].model,
            Some(Model::Wh1000Xm5),
            "chunks of {chunk_size}"
        );
        // Every request was sent only once its predecessor was ACKed, and
        // every frame from the Device was ACKed.
        let mut written = Vec::new();
        while let Ok(frame) = link.written.try_recv() {
            written.push(frame);
        }
        assert_eq!(written, sent(&records), "chunks of {chunk_size}");
    }
}

#[tokio::test]
async fn a_dropped_link_detaches_the_xm5() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    let mut link = os.next_link().await;
    replay(&mut link, &fixture("wh-1000xm5/attach")).await;
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
    // It's the same headphones while they stay Connected.
    assert_eq!(device.model, Some(Model::Wh1000Xm5));
}

#[tokio::test]
async fn every_frame_of_an_attach_is_frame_logged_like_the_fixture() {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("xm5_attach.frames");
    let _ = std::fs::remove_file(&path);
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    core.command(Command::SetFrameLog(Some(path.clone())))
        .await
        .unwrap();
    os.observe(Observation::DeviceConnected(xm5()));
    let mut link = os.next_link().await;
    let records = fixture("wh-1000xm5/attach");

    replay(&mut link, &records).await;
    wait_for(&core, |list| {
        list.devices[0].battery == Some(Battery::Headphones { level: 69 })
    })
    .await;
    core.command(Command::SetFrameLog(None)).await.unwrap();

    let log = FrameRecord::parse_all(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let summary = |records: &[FrameRecord]| -> Vec<_> {
        records
            .iter()
            .map(|r| (r.model.clone(), r.direction, r.bytes.clone()))
            .collect()
    };
    assert_eq!(summary(&log), summary(&records));
}

#[tokio::test]
async fn a_frame_log_on_from_the_start_logs_an_attach_at_startup() {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("xm5_attach_at_startup.frames");
    let _ = std::fs::remove_file(&path);
    let (platform, mut os) = fake_platform();
    // The Device is already Connected when the core starts.
    os.observe(Observation::DeviceConnected(xm5()));
    let (core, frame_log) = Core::start_with_frame_log(platform, &path);
    frame_log.unwrap();
    let mut link = os.next_link().await;
    let records = fixture("wh-1000xm5/attach");

    replay(&mut link, &records).await;
    wait_for(&core, |list| {
        list.devices[0].battery == Some(Battery::Headphones { level: 69 })
    })
    .await;
    core.command(Command::SetFrameLog(None)).await.unwrap();

    let log = FrameRecord::parse_all(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let bytes = |records: &[FrameRecord]| -> Vec<_> {
        records
            .iter()
            .map(|r| (r.direction, r.bytes.clone()))
            .collect()
    };
    assert_eq!(bytes(&log), bytes(&records));
}

/// An ACK with sequence number 0: for a frame sent with 1.
const ACK_SEQ_0: [u8; 9] = [0x3e, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x3c];
/// An ACK with sequence number 1: for a frame sent with 0.
const ACK_SEQ_1: [u8; 9] = [0x3e, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x02, 0x3c];

mod support;

use headphone_bridge_core::platform::Observation;
use headphone_bridge_core::{Battery, Command, CommandError, Core, DeviceList, DeviceState};
use support::{
    buds4pro, fake_platform, fixture, received, replay, samsung_frame, wait_for, xm5, DeviceLink,
    FakeOs,
};

/// real-nc-level-2's extended status, with byte 23 (Ambient Level) and byte
/// 24 (Noise Cancelling Level) set to `ambient` and `noise_cancelling`.
fn extended_status(ambient: u8, noise_cancelling: u8) -> Vec<u8> {
    let frame = &received(&fixture("galaxy-buds4-pro/real-nc-level-2"))[0];
    let mut status = frame[4..frame.len() - 3].to_vec();
    status[23] = ambient;
    status[24] = noise_cancelling;
    samsung_frame(0x61, &status)
}

#[tokio::test]
async fn attaching_reports_the_noise_cancelling_level_the_buds_are_at() {
    for level in 0..=4 {
        let (platform, mut os) = fake_platform();
        let core = Core::start(platform);
        os.observe(Observation::DeviceConnected(buds4pro()));
        let link = os.next_link().await;

        // Each was captured after setting the level on the phone, with
        // Ambient Level 3.
        for frame in received(&fixture(&format!("galaxy-buds4-pro/real-nc-level-{level}"))) {
            link.send.send(frame).unwrap();
        }

        let list = wait_for(&core, |list| {
            list.devices[0].state == DeviceState::Attached
                && list.devices[0].noise_cancelling_level == Some(level)
        })
        .await;
        assert_eq!(
            list.devices[0].ambient_level,
            Some(3),
            "real-nc-level-{level}"
        );
    }
}

#[tokio::test]
async fn a_level_outside_the_models_range_from_the_device_is_ignored() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    let link = os.next_link().await;
    link.send.send(extended_status(3, 2)).unwrap();
    wait_for(&core, |list| {
        list.devices[0].noise_cancelling_level == Some(2)
    })
    .await;

    link.send.send(extended_status(3, 5)).unwrap();
    // A change of Ambient Level shows each status has been handled.
    link.send.send(extended_status(1, 0xff)).unwrap();

    let list = wait_for(&core, |list| list.devices[0].ambient_level == Some(1)).await;
    assert_eq!(list.devices[0].noise_cancelling_level, Some(2));
}

/// Starts a core with a Buds4 Pro Attached at Ambient Level 3 and Noise
/// Cancelling Level 2.
async fn attached_buds4pro() -> (Core, FakeOs, DeviceLink) {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    let mut link = os.next_link().await;
    link.written.recv().await.expect("MANAGER_INFO");
    link.send.send(extended_status(3, 2)).unwrap();
    wait_for(&core, |list| {
        list.devices[0].state == DeviceState::Attached
            && list.devices[0].noise_cancelling_level == Some(2)
    })
    .await;
    (core, os, link)
}

/// `NOISE_REDUCTION_LEVEL` (0x83) with level 4.
const SET_NOISE_CANCELLING_LEVEL_4: [u8; 8] = [0xfd, 0x04, 0x00, 0x83, 0x04, 0x4f, 0x0e, 0xdd];
const MANAGER_INFO: [u8; 10] = [0xfd, 0x06, 0x00, 0x88, 0x01, 0x01, 0x22, 0xda, 0x58, 0xdd];

/// A status that changes the battery, to show earlier frames have been handled.
async fn settle(core: &Core, link: &DeviceLink) -> DeviceList {
    link.send
        .send(samsung_frame(
            0x60,
            &[0x01, 0x50, 0x50, 0x01, 0x01, 0x11, 0x00, 0x00],
        ))
        .unwrap();
    wait_for(core, |list| {
        matches!(
            list.devices[0].battery,
            Some(Battery::Buds { left: Some(80), .. })
        )
    })
    .await
}

#[tokio::test]
async fn setting_a_level_sends_the_level_set_and_changes_nothing_until_the_device_confirms() {
    let (core, _os, mut link) = attached_buds4pro().await;

    core.command(Command::SetNoiseCancellingLevel(buds4pro().address, 4))
        .await
        .unwrap();

    assert_eq!(
        link.written.recv().await.unwrap(),
        SET_NOISE_CANCELLING_LEVEL_4
    );
    // Like an Ambient Level set, polled with MANAGER_INFO for a fresh
    // extended status.
    assert_eq!(link.written.recv().await.unwrap(), MANAGER_INFO);
    let list = settle(&core, &link).await;
    assert_eq!(list.devices[0].noise_cancelling_level, Some(2));

    link.send.send(extended_status(3, 4)).unwrap();
    wait_for(&core, |list| {
        list.devices[0].noise_cancelling_level == Some(4)
    })
    .await;
}

#[tokio::test]
async fn an_ack_confirms_the_level_only_when_it_carries_the_one_asked_for() {
    let (core, _os, link) = attached_buds4pro().await;

    core.command(Command::SetNoiseCancellingLevel(buds4pro().address, 0))
        .await
        .unwrap();
    // An ACK with any other value, such as one for an earlier set, is only
    // a receipt.
    link.send.send(samsung_frame(0x42, &[0x83, 0x03])).unwrap();
    let list = settle(&core, &link).await;
    assert_eq!(list.devices[0].noise_cancelling_level, Some(2));

    link.send.send(samsung_frame(0x42, &[0x83, 0x00])).unwrap();

    wait_for(&core, |list| {
        list.devices[0].noise_cancelling_level == Some(0)
    })
    .await;
}

#[tokio::test]
async fn overlapping_ambient_and_noise_cancelling_level_sets_each_confirm_only_their_own() {
    let (core, _os, link) = attached_buds4pro().await;

    core.command(Command::SetAmbientLevel(buds4pro().address, 1))
        .await
        .unwrap();
    core.command(Command::SetNoiseCancellingLevel(buds4pro().address, 1))
        .await
        .unwrap();
    // An ACK for one level carrying the value asked for the other confirms
    // neither.
    link.send.send(samsung_frame(0x42, &[0x84, 0x04])).unwrap();
    link.send.send(samsung_frame(0x42, &[0x83, 0x04])).unwrap();
    let list = settle(&core, &link).await;
    assert_eq!(list.devices[0].ambient_level, Some(3));
    assert_eq!(list.devices[0].noise_cancelling_level, Some(2));

    link.send.send(samsung_frame(0x42, &[0x83, 0x01])).unwrap();
    let list = wait_for(&core, |list| {
        list.devices[0].noise_cancelling_level == Some(1)
    })
    .await;
    assert_eq!(list.devices[0].ambient_level, Some(3));

    link.send.send(samsung_frame(0x42, &[0x84, 0x01])).unwrap();
    let list = wait_for(&core, |list| list.devices[0].ambient_level == Some(1)).await;
    assert_eq!(list.devices[0].noise_cancelling_level, Some(1));
}

#[tokio::test]
async fn a_level_past_the_models_range_is_rejected_before_anything_is_sent() {
    let (core, _os, mut link) = attached_buds4pro().await;

    let result = core
        .command(Command::SetNoiseCancellingLevel(buds4pro().address, 5))
        .await;

    assert_eq!(result, Err(CommandError::OutOfRange));
    drop(core);
    assert_eq!(link.written.recv().await, None, "nothing else written");
}

#[tokio::test]
async fn a_wh1000xm5_has_no_noise_cancelling_level_to_set() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    let mut link = os.next_link().await;
    replay(&mut link, &fixture("wh-1000xm5/attach")).await;
    wait_for(&core, |list| list.devices[0].ambient_level.is_some()).await;
    while link.written.try_recv().is_ok() {}

    let result = core
        .command(Command::SetNoiseCancellingLevel(xm5().address, 2))
        .await;

    assert_eq!(result, Err(CommandError::MissingCapability));
    drop(core);
    assert_eq!(link.written.recv().await, None, "nothing else written");
}

#[tokio::test]
async fn real_buds4pro_level_sets_are_confirmed_by_their_acks() {
    use headphone_bridge_core::frame_log::Direction;
    use headphone_bridge_core::NoiseControlMode::{self, *};
    enum Set {
        Level(u8),
        Mode(NoiseControlMode),
    }
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    let mut link = os.next_link().await;
    // The Device ACKs each level set with `83 <applied level>` and answers
    // the poll with a bare MANAGER_INFO, so the ACK must confirm it.
    let records = fixture("galaxy-buds4-pro/real-nc-level-set");
    let set_at: Vec<usize> = records
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            r.direction == Direction::Sent && matches!(r.bytes.get(3), Some(0x83 | 0x78))
        })
        .map(|(i, _)| i)
        .collect();
    let sets = [
        Set::Level(4),
        Set::Level(3),
        Set::Level(2),
        Set::Mode(Ambient),
        Set::Mode(NoiseCancelling),
        Set::Level(3),
        Set::Level(4),
        Set::Level(0),
        Set::Level(4),
    ];
    assert_eq!(set_at.len(), sets.len());

    replay(&mut link, &records[..set_at[0]]).await;
    wait_for(&core, |list| {
        list.devices[0].chosen_mode == Some(NoiseCancelling)
            && list.devices[0].noise_cancelling_level == Some(0)
    })
    .await;
    let mut level = 0;
    for (i, set) in sets.into_iter().enumerate() {
        let end = set_at.get(i + 1).copied().unwrap_or(records.len());
        match set {
            Set::Level(asked) => {
                core.command(Command::SetNoiseCancellingLevel(buds4pro().address, asked))
                    .await
                    .unwrap();
                assert_eq!(
                    core.device_list().devices[0].noise_cancelling_level,
                    Some(level),
                    "level {asked} before its ACK"
                );
                replay(&mut link, &records[set_at[i]..end]).await;
                level = asked;
            }
            Set::Mode(mode) => {
                core.command(Command::SetNoiseControlMode(buds4pro().address, mode))
                    .await
                    .unwrap();
                replay(&mut link, &records[set_at[i]..end]).await;
                wait_for(&core, |list| list.devices[0].chosen_mode == Some(mode)).await;
            }
        }
        wait_for(&core, |list| {
            list.devices[0].noise_cancelling_level == Some(level)
        })
        .await;
    }
}

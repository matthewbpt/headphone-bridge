//! The Galaxy Buds2, from a synthesised Attach (`fixtures/galaxy-buds2/
//! attach.frames`) and the `real-*` fixtures from a frame log.

mod support;

use std::time::Duration;

use headphone_bridge_core::frame_log::{Direction, FrameRecord};
use headphone_bridge_core::platform::{DeviceAddress, DeviceIdentity, Observation};
use headphone_bridge_core::{
    Battery, Capabilities, Command, CommandError, Core, DeviceState, EffectiveModeReason,
    LevelRange, Model, NoiseControlMode, NotAttachedReason,
};
use support::{
    buds2, fake_platform, fixture, received, replay, samsung_frame, wait_for, DeviceLink, FakeOs,
    SAMSUNG_SERVICE,
};

const MANAGER_INFO: [u8; 10] = [0xfd, 0x06, 0x00, 0x88, 0x01, 0x01, 0x22, 0xda, 0x58, 0xdd];

/// Starts a core with a Buds2 Connected, and returns its link once the
/// handshake has been written.
async fn connected_buds2() -> (Core, FakeOs, DeviceLink) {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(buds2()));
    let mut link = os.next_link().await;
    assert_eq!(link.written.recv().await.unwrap(), MANAGER_INFO);
    (core, os, link)
}

#[tokio::test]
async fn a_buds2_is_sent_the_handshake_and_attached_as_a_galaxy_buds2() {
    let (core, _os, link) = connected_buds2().await;

    link.send
        .send(received(&fixture("galaxy-buds2/attach"))[0].clone())
        .unwrap();

    let list = wait_for(&core, |list| list.devices[0].state == DeviceState::Attached).await;
    let device = &list.devices[0];
    assert_eq!(device.model, Some(Model::GalaxyBuds2));
    assert_eq!(
        device.capabilities,
        Capabilities {
            noise_control_modes: vec![
                NoiseControlMode::Off,
                NoiseControlMode::NoiseCancelling,
                NoiseControlMode::Ambient,
            ],
            ambient_level: Some(LevelRange { min: 0, max: 2 }),
            noise_cancelling_level: None,
            power_off: false,
        }
    );
}

/// The Buds2 extended status in `galaxy-buds2/attach`, as a frame with
/// `edit` applied to its payload.
fn buds2_extended_status(edit: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
    let frame = &received(&fixture("galaxy-buds2/attach"))[0];
    let mut payload = frame[4..frame.len() - 3].to_vec();
    edit(&mut payload);
    samsung_frame(0x61, &payload)
}

#[tokio::test]
async fn a_buds2_named_device_reporting_another_models_device_id_is_unsupported() {
    // 360, a white Buds4 Pro, and 14337, which GalaxyBudsClient lists as an
    // unknown Buds2.
    for device_id in [360u16, 14337] {
        let (core, _os, mut link) = connected_buds2().await;

        link.send
            .send(buds2_extended_status(|status| {
                status[14..16].copy_from_slice(&device_id.to_le_bytes());
                status[16..18].copy_from_slice(&device_id.to_le_bytes());
            }))
            .unwrap();

        let list = wait_for(&core, |list| {
            list.devices[0].state != DeviceState::Attaching
        })
        .await;
        assert_eq!(
            list.devices[0].state,
            DeviceState::NotAttached {
                reason: NotAttachedReason::UnsupportedModel
            },
            "device id {device_id}"
        );
        assert_eq!(list.devices[0].model, None);
        assert_eq!(
            link.written.recv().await,
            None,
            "the link must close with nothing else written"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn a_galaxy_buds2_pro_is_listed_as_unsupported_and_sent_nothing() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    let buds2pro = DeviceIdentity {
        address: DeviceAddress("10:20:30:40:50:60".into()),
        name: "Galaxy Buds2 Pro (9F3C)".into(),
        service_uuids: vec![SAMSUNG_SERVICE],
    };

    os.observe(Observation::DeviceConnected(buds2pro));

    let list = wait_for(&core, |list| list.devices.len() == 1).await;
    assert_eq!(
        list.devices[0].state,
        DeviceState::NotAttached {
            reason: NotAttachedReason::UnsupportedModel
        }
    );
    assert!(
        !os.attempts_within(Duration::from_secs(600)).await,
        "no control link is opened, so nothing is written"
    );
}

/// Starts a core with a Buds2 Attached through `galaxy-buds2/attach`: both
/// buds in the ear, Noise Cancelling, Ambient Level 1.
async fn attached_buds2() -> (Core, FakeOs, DeviceLink) {
    let (core, os, link) = connected_buds2().await;
    link.send
        .send(received(&fixture("galaxy-buds2/attach"))[0].clone())
        .unwrap();
    wait_for(&core, |list| {
        list.devices[0].state == DeviceState::Attached
            && list.devices[0].chosen_mode == Some(NoiseControlMode::NoiseCancelling)
    })
    .await;
    (core, os, link)
}

#[tokio::test]
async fn the_extended_status_reports_the_chosen_mode_and_ambient_level() {
    let (core, _os, _link) = attached_buds2().await;

    let list = wait_for(&core, |list| list.devices[0].ambient_level == Some(1)).await;
    let device = &list.devices[0];
    assert_eq!(device.chosen_mode, Some(NoiseControlMode::NoiseCancelling));
    assert_eq!(
        device.effective_mode,
        Some(NoiseControlMode::NoiseCancelling)
    );
    assert_eq!(device.noise_cancelling_level, None);
}

#[tokio::test]
async fn a_mode_paused_by_a_bud_out_of_the_ear_is_only_the_effective_mode() {
    let (core, _os, link) = attached_buds2().await;

    // The left bud out of the ear (placement 0x21), and the Device pauses
    // Noise Cancelling to Off, as a Buds4 Pro does (capture-1c).
    link.send
        .send(buds2_extended_status(|status| {
            status[6] = 0x21;
            status[12] = 0x00;
        }))
        .unwrap();

    let list = wait_for(&core, |list| {
        list.devices[0].effective_mode == Some(NoiseControlMode::Off)
    })
    .await;
    let device = &list.devices[0];
    assert_eq!(device.chosen_mode, Some(NoiseControlMode::NoiseCancelling));
    assert_eq!(
        device.effective_mode_reason,
        Some(EffectiveModeReason::BudOutOfEar)
    );
}

/// `NOISE_CONTROLS` (0x78) with Ambient (2).
const SET_AMBIENT: [u8; 8] = [0xfd, 0x04, 0x00, 0x78, 0x02, 0xb2, 0xa1, 0xdd];
/// `AMBIENT_VOLUME` (0x84) with level 2.
const SET_AMBIENT_LEVEL_2: [u8; 8] = [0xfd, 0x04, 0x00, 0x84, 0x02, 0x1e, 0xf7, 0xdd];

#[tokio::test]
async fn a_mode_set_sends_noise_controls_then_manager_info_and_an_ack_confirms_it() {
    let (core, _os, mut link) = attached_buds2().await;

    core.command(Command::SetNoiseControlMode(
        buds2().address,
        NoiseControlMode::Ambient,
    ))
    .await
    .unwrap();

    assert_eq!(link.written.recv().await.unwrap(), SET_AMBIENT);
    assert_eq!(link.written.recv().await.unwrap(), MANAGER_INFO);
    // Its ACK echoes the applied mode, as a Buds4 Pro's does
    // (real-noise-control).
    link.send.send(samsung_frame(0x42, &[0x78, 0x02])).unwrap();
    wait_for(&core, |list| {
        list.devices[0].chosen_mode == Some(NoiseControlMode::Ambient)
    })
    .await;
}

#[tokio::test]
async fn a_mode_set_is_confirmed_by_the_next_extended_status_without_an_ack() {
    let (core, _os, mut link) = attached_buds2().await;

    core.command(Command::SetNoiseControlMode(
        buds2().address,
        NoiseControlMode::Off,
    ))
    .await
    .unwrap();
    link.written.recv().await.expect("NOISE_CONTROLS");
    assert_eq!(link.written.recv().await.unwrap(), MANAGER_INFO);
    link.send
        .send(buds2_extended_status(|status| status[12] = 0x00))
        .unwrap();

    let list = wait_for(&core, |list| {
        list.devices[0].chosen_mode == Some(NoiseControlMode::Off)
    })
    .await;
    assert_eq!(list.devices[0].effective_mode_reason, None);
}

#[tokio::test]
async fn an_ambient_level_set_sends_ambient_volume_then_manager_info_and_an_ack_confirms_it() {
    let (core, _os, mut link) = attached_buds2().await;

    core.command(Command::SetAmbientLevel(buds2().address, 2))
        .await
        .unwrap();

    assert_eq!(link.written.recv().await.unwrap(), SET_AMBIENT_LEVEL_2);
    assert_eq!(link.written.recv().await.unwrap(), MANAGER_INFO);
    link.send.send(samsung_frame(0x42, &[0x84, 0x02])).unwrap();
    wait_for(&core, |list| list.devices[0].ambient_level == Some(2)).await;
}

#[tokio::test]
async fn an_ambient_level_set_is_confirmed_by_the_next_extended_status_without_an_ack() {
    let (core, _os, mut link) = attached_buds2().await;

    core.command(Command::SetAmbientLevel(buds2().address, 0))
        .await
        .unwrap();
    link.written.recv().await.expect("AMBIENT_VOLUME");
    assert_eq!(link.written.recv().await.unwrap(), MANAGER_INFO);
    link.send
        .send(buds2_extended_status(|status| status[23] = 0))
        .unwrap();

    wait_for(&core, |list| list.devices[0].ambient_level == Some(0)).await;
}

#[tokio::test]
async fn an_ambient_level_beyond_the_buds2s_three_steps_is_rejected_before_anything_is_sent() {
    let (core, _os, mut link) = attached_buds2().await;

    let result = core
        .command(Command::SetAmbientLevel(buds2().address, 3))
        .await;

    assert_eq!(result, Err(CommandError::OutOfRange));
    drop(core);
    assert_eq!(link.written.recv().await, None, "nothing else written");
}

#[tokio::test]
async fn adaptive_is_never_sent_to_a_buds2() {
    let (core, _os, mut link) = attached_buds2().await;

    let result = core
        .command(Command::SetNoiseControlMode(
            buds2().address,
            NoiseControlMode::Adaptive,
        ))
        .await;

    assert_eq!(result, Err(CommandError::MissingCapability));
    drop(core);
    assert_eq!(link.written.recv().await, None, "nothing else written");
}

#[tokio::test]
async fn a_short_noise_controls_update_is_a_gesture_that_sets_the_chosen_mode() {
    let (core, _os, link) = attached_buds2().await;

    // Too short to carry a cause (byte 4, as in real-gesture): one byte (the
    // mode) or two (with the placement, both buds in), as GalaxyBudsClient
    // decodes them.
    link.send.send(samsung_frame(0x77, &[0x02])).unwrap();
    wait_for(&core, |list| {
        list.devices[0].chosen_mode == Some(NoiseControlMode::Ambient)
    })
    .await;
    link.send.send(samsung_frame(0x77, &[0x00, 0x11])).unwrap();
    wait_for(&core, |list| {
        list.devices[0].chosen_mode == Some(NoiseControlMode::Off)
    })
    .await;
}

#[tokio::test]
async fn a_charging_bit_in_a_revision_10_status_docks_that_bud_and_shows_the_case_level() {
    let (core, _os, link) = attached_buds2().await;

    // Both buds out of the ear (0x22), so the case level (51 %) is stale.
    link.send
        .send(buds2_extended_status(|status| status[6] = 0x22))
        .unwrap();
    wait_for(&core, |list| {
        list.devices[0].battery
            == Some(Battery::Buds {
                left: Some(100),
                right: Some(100),
                case: None,
            })
    })
    .await;
    // The right bud's charging bit (0x04 at byte 36) is set before its
    // placement says it is in the case: the case level is live.
    link.send
        .send(buds2_extended_status(|status| {
            status[6] = 0x22;
            status[36] = 0x04;
        }))
        .unwrap();

    wait_for(&core, |list| {
        list.devices[0].battery
            == Some(Battery::Buds {
                left: Some(100),
                right: Some(100),
                case: Some(51),
            })
    })
    .await;
}

#[tokio::test]
async fn a_status_before_revision_10_updates_battery_from_placement_alone() {
    let (core, _os, link) = attached_buds2().await;

    // Before revision 10, byte 36 isn't the charging bits: a right bud out of
    // the ear (0x22) stays out, and the case level stays unknown.
    link.send
        .send(buds2_extended_status(|status| {
            status[0] = 9;
            status[2] = 80;
            status[6] = 0x22;
            status[36] = 0x04;
        }))
        .unwrap();
    wait_for(&core, |list| {
        list.devices[0].battery
            == Some(Battery::Buds {
                left: Some(80),
                right: Some(100),
                case: None,
            })
    })
    .await;
    // Once its placement puts the right bud in the case (0x23), the case level
    // is live.
    link.send
        .send(buds2_extended_status(|status| {
            status[0] = 9;
            status[2] = 80;
            status[6] = 0x23;
        }))
        .unwrap();

    wait_for(&core, |list| {
        list.devices[0].battery
            == Some(Battery::Buds {
                left: Some(80),
                right: Some(100),
                case: Some(51),
            })
    })
    .await;
}

#[tokio::test]
async fn a_seven_byte_status_update_from_older_firmware_updates_battery() {
    let (core, _os, link) = attached_buds2().await;

    // No charging byte: L 80 %, R 70 %, the right bud in the case (0x13) by
    // placement, case 64 %.
    link.send
        .send(samsung_frame(
            0x60,
            &[0x01, 0x50, 0x46, 0x01, 0x01, 0x13, 0x40],
        ))
        .unwrap();

    wait_for(&core, |list| {
        list.devices[0].battery
            == Some(Battery::Buds {
                left: Some(80),
                right: Some(70),
                case: Some(64),
            })
    })
    .await;
}

#[tokio::test]
async fn frame_logs_label_the_buds2s_frames_galaxy_buds2() {
    let path = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("buds2.frames");
    let _ = std::fs::remove_file(&path);
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    core.command(Command::SetFrameLog(Some(path.clone())))
        .await
        .unwrap();

    os.observe(Observation::DeviceConnected(buds2()));
    let link = os.next_link().await;
    link.send
        .send(received(&fixture("galaxy-buds2/attach"))[0].clone())
        .unwrap();
    wait_for(&core, |list| list.devices[0].state == DeviceState::Attached).await;
    core.command(Command::SetFrameLog(None)).await.unwrap();

    let log = FrameRecord::parse_all(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let models: Vec<&str> = log.iter().map(|record| record.model.as_str()).collect();
    assert_eq!(models, ["samsung", "galaxy-buds2"]);
}

#[tokio::test]
async fn a_real_buds2_attach_reports_its_mode_level_and_battery() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(buds2()));
    let mut link = os.next_link().await;

    replay(&mut link, &fixture("galaxy-buds2/real-attach")).await;

    let list = wait_for(&core, |list| list.devices[0].battery.is_some()).await;
    let device = &list.devices[0];
    assert_eq!(device.state, DeviceState::Attached);
    assert_eq!(device.model, Some(Model::GalaxyBuds2));
    assert_eq!(device.chosen_mode, Some(NoiseControlMode::Ambient));
    assert_eq!(device.effective_mode, Some(NoiseControlMode::Ambient));
    assert_eq!(device.effective_mode_reason, None);
    assert_eq!(device.ambient_level, Some(1));
    assert_eq!(device.noise_cancelling_level, None);
    assert_eq!(
        device.battery,
        Some(Battery::Buds {
            left: Some(99),
            right: Some(99),
            case: None,
        })
    );
    drop(core);
    assert_eq!(link.written.recv().await, None, "only MANAGER_INFO written");
}

/// The indices of the sets the app sent with message `id` in `records`.
fn sets_in(records: &[FrameRecord], id: u8) -> Vec<usize> {
    records
        .iter()
        .enumerate()
        .filter(|(_, r)| r.direction == Direction::Sent && r.bytes.get(3) == Some(&id))
        .map(|(i, _)| i)
        .collect()
}

#[tokio::test]
async fn real_buds2_mode_sets_are_confirmed_by_their_acks_and_a_bud_out_only_pauses_the_mode() {
    use NoiseControlMode::*;
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(buds2()));
    let mut link = os.next_link().await;
    // The Device ACKs each set with `78 <applied mode>` and answers the poll
    // with a bare MANAGER_INFO, so the ACK must confirm it.
    let records = fixture("galaxy-buds2/real-noise-control");
    let set_at = sets_in(&records, 0x78);
    let modes = [NoiseCancelling, Ambient, Off, Ambient];
    assert_eq!(set_at.len(), modes.len());
    // The bud-out and bud-in updates follow the last set's replies.
    let bud_out_at = set_at[3] + 5;

    replay(&mut link, &records[..set_at[0]]).await;
    wait_for(&core, |list| list.devices[0].chosen_mode == Some(Ambient)).await;
    for (i, mode) in modes.into_iter().enumerate() {
        core.command(Command::SetNoiseControlMode(buds2().address, mode))
            .await
            .unwrap();
        let end = set_at.get(i + 1).copied().unwrap_or(bud_out_at);
        replay(&mut link, &records[set_at[i]..end]).await;
        wait_for(&core, |list| list.devices[0].chosen_mode == Some(mode)).await;
    }

    // A bud leaves the ear, and the Buds2 pauses Ambient to Off.
    replay(&mut link, &records[bud_out_at..bud_out_at + 4]).await;
    let list = wait_for(&core, |list| list.devices[0].effective_mode == Some(Off)).await;
    let device = &list.devices[0];
    assert_eq!(device.chosen_mode, Some(Ambient));
    assert_eq!(
        device.effective_mode_reason,
        Some(EffectiveModeReason::BudOutOfEar)
    );
    // It goes back in, and Ambient resumes.
    replay(&mut link, &records[bud_out_at + 4..]).await;
    let list = wait_for(&core, |list| {
        list.devices[0].effective_mode == Some(Ambient)
    })
    .await;
    let device = &list.devices[0];
    assert_eq!(device.chosen_mode, Some(Ambient));
    assert_eq!(device.effective_mode_reason, None);
    drop(core);
    assert_eq!(link.written.recv().await, None, "nothing else written");
}

#[tokio::test]
async fn real_buds2_ambient_level_sets_down_to_0_are_confirmed_by_their_acks() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(buds2()));
    let mut link = os.next_link().await;
    // The Device ACKs each set with `84 <applied level>` and answers the poll
    // with a bare MANAGER_INFO, so the ACK must confirm it.
    let records = fixture("galaxy-buds2/real-ambient-level");
    let set_at = sets_in(&records, 0x84);
    let levels = [2, 1, 0, 1, 2, 1];
    assert_eq!(set_at.len(), levels.len());

    replay(&mut link, &records[..set_at[0]]).await;
    wait_for(&core, |list| list.devices[0].ambient_level == Some(1)).await;
    for (i, level) in levels.into_iter().enumerate() {
        core.command(Command::SetAmbientLevel(buds2().address, level))
            .await
            .unwrap();
        let end = set_at.get(i + 1).copied().unwrap_or(records.len());
        replay(&mut link, &records[set_at[i]..end]).await;
        wait_for(&core, |list| list.devices[0].ambient_level == Some(level)).await;
    }
    let list = core.device_list();
    assert_eq!(list.devices[0].chosen_mode, Some(NoiseControlMode::Ambient));
}

#[tokio::test]
async fn real_buds2_long_presses_each_set_the_chosen_mode_and_nothing_is_written() {
    use NoiseControlMode::*;
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(buds2()));
    let mut link = os.next_link().await;
    let records = fixture("galaxy-buds2/real-gesture");
    let press_at: Vec<usize> = records
        .iter()
        .enumerate()
        .filter(|(_, r)| r.bytes.get(3) == Some(&0x77))
        .map(|(i, _)| i)
        .collect();
    let modes = [NoiseCancelling, Ambient, NoiseCancelling, Ambient];
    assert_eq!(press_at.len(), modes.len());

    replay(&mut link, &records[..press_at[0]]).await;
    wait_for(&core, |list| list.devices[0].chosen_mode == Some(Ambient)).await;
    for (i, mode) in modes.into_iter().enumerate() {
        let end = press_at.get(i + 1).copied().unwrap_or(records.len());
        replay(&mut link, &records[press_at[i]..end]).await;
        let list = wait_for(&core, |list| list.devices[0].chosen_mode == Some(mode)).await;
        assert_eq!(list.devices[0].effective_mode, Some(mode));
    }
    drop(core);
    assert_eq!(link.written.recv().await, None, "only MANAGER_INFO written");
}

#[tokio::test]
async fn a_buds2_placement_update_with_both_buds_in_is_only_the_effective_mode() {
    let (core, _os, link) = attached_buds2().await;

    // Cause 2 at byte 4 is a placement change, as in real-noise-control, so
    // the Off it reports is not chosen, even with both buds in (0x11).
    link.send
        .send(samsung_frame(0x77, &[0x00, 0x11, 0x01, 0x66, 0x02]))
        .unwrap();

    let list = wait_for(&core, |list| {
        list.devices[0].effective_mode == Some(NoiseControlMode::Off)
    })
    .await;
    assert_eq!(
        list.devices[0].chosen_mode,
        Some(NoiseControlMode::NoiseCancelling)
    );
}

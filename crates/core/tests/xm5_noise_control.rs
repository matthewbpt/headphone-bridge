#![cfg(test)]

mod support;

use std::time::Duration;

use headphone_bridge_core::frame_log::Direction;
use headphone_bridge_core::platform::Observation;
use headphone_bridge_core::{Battery, CommandError, CommandKind, Core, NoiseControlMode};
use support::{
    device_command, fake_platform, fixture, replay, sony_frame, wait_for, with_equalizer_read, xm5,
    DeviceLink, FakeOs,
};

/// Starts a core with a WH-1000XM5 Attached through the whole attach
/// fixture, which reports Ambient, Focus on Voice on and Ambient Level 10.
async fn attached_xm5() -> (Core, FakeOs, DeviceLink) {
    let (platform, mut os) = fake_platform();
    let core = Core::builder().start(platform);
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

    core.command(device_command(
        xm5().address,
        CommandKind::SetNoiseControlMode {
            mode: NoiseControlMode::NoiseCancelling,
        },
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
    link.send
        .send(sony_frame(0, &[0x25, 0x00, 68, 0x00]))
        .unwrap();
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

#[tokio::test]
async fn setting_a_level_resends_the_mode_and_focus_on_voice_and_waits_for_the_device() {
    let (core, _os, mut link) = attached_xm5().await;

    core.command(device_command(
        xm5().address,
        CommandKind::SetAmbientLevel { level: 15 },
    ))
    .await
    .unwrap();

    assert_eq!(
        written(&mut link).await,
        sony_frame(1, &[0x68, 0x17, 0x01, 0x01, 0x01, 0x01, 0x0f])
    );
    link.send.send(ACK_SEQ_0.to_vec()).unwrap();
    link.send
        .send(sony_frame(0, &[0x25, 0x00, 68, 0x00]))
        .unwrap();
    let list = wait_for(&core, |list| {
        list.devices[0].battery == Some(Battery::Headphones { level: 68 })
    })
    .await;
    assert_eq!(list.devices[0].ambient_level, Some(10));

    link.send
        .send(sony_frame(1, &[0x69, 0x17, 0x01, 0x01, 0x01, 0x01, 0x0f]))
        .unwrap();
    wait_for(&core, |list| list.devices[0].ambient_level == Some(15)).await;
}

#[tokio::test]
async fn a_set_made_before_the_last_is_confirmed_keeps_the_last() {
    let (core, _os, mut link) = attached_xm5().await;

    core.command(device_command(
        xm5().address,
        CommandKind::SetNoiseControlMode {
            mode: NoiseControlMode::Off,
        },
    ))
    .await
    .unwrap();
    core.command(device_command(
        xm5().address,
        CommandKind::SetAmbientLevel { level: 3 },
    ))
    .await
    .unwrap();

    // Off keeps the mode byte, ambient, as it was.
    assert_eq!(
        written(&mut link).await,
        sony_frame(1, &[0x68, 0x17, 0x01, 0x00, 0x01, 0x01, 0x0a])
    );
    // The level set waits for the ACK, and doesn't switch Off back on.
    link.send.send(ACK_SEQ_0.to_vec()).unwrap();
    assert_eq!(
        written(&mut link).await,
        sony_frame(0, &[0x68, 0x17, 0x01, 0x00, 0x01, 0x01, 0x03])
    );
}

#[tokio::test]
async fn adaptive_is_rejected_and_nothing_is_sent() {
    let (core, _os, mut link) = attached_xm5().await;

    assert_eq!(
        core.command(device_command(
            xm5().address,
            CommandKind::SetNoiseControlMode {
                mode: NoiseControlMode::Adaptive
            }
        ))
        .await,
        Err(CommandError::MissingCapability)
    );
    // A later set is the next thing written.
    core.command(device_command(
        xm5().address,
        CommandKind::SetAmbientLevel { level: 20 },
    ))
    .await
    .unwrap();
    assert_eq!(
        written(&mut link).await,
        sony_frame(1, &[0x68, 0x17, 0x01, 0x01, 0x01, 0x01, 0x14])
    );
}

#[tokio::test]
async fn levels_outside_1_to_20_are_rejected() {
    let (core, _os, _link) = attached_xm5().await;

    for level in [0, 21] {
        assert_eq!(
            core.command(device_command(
                xm5().address,
                CommandKind::SetAmbientLevel { level }
            ))
            .await,
            Err(CommandError::OutOfRange)
        );
    }
}

#[tokio::test]
async fn changes_from_the_phone_appear_live() {
    let (core, _os, mut link) = attached_xm5().await;

    // The Sony app switches to Noise Cancelling, then Off.
    link.send
        .send(sony_frame(0, &[0x69, 0x17, 0x01, 0x01, 0x00, 0x01, 0x0a]))
        .unwrap();
    wait_for(&core, |list| {
        list.devices[0].chosen_mode == Some(NoiseControlMode::NoiseCancelling)
    })
    .await;
    link.send
        .send(sony_frame(1, &[0x69, 0x17, 0x01, 0x00, 0x00, 0x01, 0x0a]))
        .unwrap();
    wait_for(&core, |list| {
        list.devices[0].chosen_mode == Some(NoiseControlMode::Off)
    })
    .await;
    // Then Ambient, mid-drag of its slider at level 4.
    link.send
        .send(sony_frame(0, &[0x69, 0x17, 0x00, 0x01, 0x01, 0x01, 0x04]))
        .unwrap();
    let list = wait_for(&core, |list| list.devices[0].ambient_level == Some(4)).await;
    assert_eq!(list.devices[0].chosen_mode, Some(NoiseControlMode::Ambient));
    // Every notification was ACKed.
    for _ in 0..3 {
        assert_eq!(written(&mut link).await[1], 0x01, "an ACK");
    }
}

#[tokio::test]
async fn a_reported_level_outside_1_to_20_is_ignored() {
    let (core, _os, link) = attached_xm5().await;

    link.send
        .send(sony_frame(0, &[0x69, 0x17, 0x01, 0x01, 0x00, 0x01, 0x00]))
        .unwrap();

    let list = wait_for(&core, |list| {
        list.devices[0].chosen_mode == Some(NoiseControlMode::NoiseCancelling)
    })
    .await;
    assert_eq!(list.devices[0].ambient_level, Some(10));
}

#[tokio::test]
async fn real_xm5_sets_replay_from_their_frame_log() {
    use NoiseControlMode::*;
    let (platform, mut os) = fake_platform();
    let core = Core::builder().start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    let mut link = os.next_link().await;
    // Recorded before the Equalizer was read on Attach, so re-based onto an
    // Attach that reads it: available, Off, flat.
    let records = with_equalizer_read(&fixture("wh-1000xm5/real-noise-control"), &FLAT_OFF);
    let address = xm5().address;
    // Each set as the capture made it, and what the Device then reported.
    let sets = [
        (
            device_command(address, CommandKind::SetNoiseControlMode { mode: Off }),
            Off,
            10,
        ),
        (
            device_command(
                address,
                CommandKind::SetNoiseControlMode {
                    mode: NoiseCancelling,
                },
            ),
            NoiseCancelling,
            10,
        ),
        (
            device_command(address, CommandKind::SetNoiseControlMode { mode: Ambient }),
            Ambient,
            10,
        ),
        (
            device_command(address, CommandKind::SetAmbientLevel { level: 15 }),
            Ambient,
            15,
        ),
        (
            device_command(address, CommandKind::SetAmbientLevel { level: 20 }),
            Ambient,
            20,
        ),
        (
            device_command(address, CommandKind::SetAmbientLevel { level: 1 }),
            Ambient,
            1,
        ),
        (
            device_command(address, CommandKind::SetAmbientLevel { level: 10 }),
            Ambient,
            10,
        ),
        (
            device_command(
                address,
                CommandKind::SetNoiseControlMode {
                    mode: NoiseCancelling,
                },
            ),
            NoiseCancelling,
            10,
        ),
    ];
    // The capture split before each NCASM_SET_PARAM the app sent.
    let set_at: Vec<usize> = records
        .iter()
        .enumerate()
        .filter(|(_, r)| r.direction == Direction::Sent && r.bytes.get(7) == Some(&0x68))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(set_at.len(), sets.len());

    replay(&mut link, &records[..set_at[0]]).await;
    let mut reported = (NoiseCancelling, 10);
    for (i, (command, mode, level)) in sets.into_iter().enumerate() {
        wait_for(&core, |list| {
            (list.devices[0].chosen_mode, list.devices[0].ambient_level)
                == (Some(reported.0), Some(reported.1))
        })
        .await;
        core.command(command).await.unwrap();
        let end = set_at.get(i + 1).copied().unwrap_or(records.len());
        replay(&mut link, &records[set_at[i]..end]).await;
        reported = (mode, level);
    }
    wait_for(&core, |list| {
        (list.devices[0].chosen_mode, list.devices[0].ambient_level)
            == (Some(NoiseCancelling), Some(10))
    })
    .await;
}

/// `EQEBB_RET_PARAM`: Off, Clear Bass and every Band at 0.
const FLAT_OFF: [u8; 10] = [0x57, 0x00, 0x00, 0x06, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a];

/// The next frame the core writes.
async fn written(link: &mut DeviceLink) -> Vec<u8> {
    tokio::time::timeout(Duration::from_secs(5), link.written.recv())
        .await
        .expect("timed out waiting for the core to write")
        .expect("link closed")
}

/// An ACK with sequence number 0: for a frame sent with 1.
const ACK_SEQ_0: [u8; 9] = [0x3e, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x3c];

#[tokio::test]
async fn sets_before_the_device_reports_its_noise_control_are_rejected() {
    let (platform, mut os) = fake_platform();
    let core = Core::builder().start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    let mut link = os.next_link().await;
    // Identified and battery read, but its NCASM params not yet asked for.
    let records = fixture("wh-1000xm5/attach");
    replay(&mut link, &records[..16]).await;
    wait_for(&core, |list| list.devices[0].battery.is_some()).await;

    assert_eq!(
        core.command(device_command(
            xm5().address,
            CommandKind::SetNoiseControlMode {
                mode: NoiseControlMode::NoiseCancelling
            }
        ))
        .await,
        Err(CommandError::SettingsNotReported)
    );
    assert_eq!(
        core.command(device_command(
            xm5().address,
            CommandKind::SetAmbientLevel { level: 15 }
        ))
        .await,
        Err(CommandError::SettingsNotReported)
    );

    // Nothing was sent: the rest of the Attach plays out as recorded, and
    // the first set after it is the next thing written.
    replay(&mut link, &records[16..]).await;
    wait_for(&core, |list| list.devices[0].ambient_level.is_some()).await;
    core.command(device_command(
        xm5().address,
        CommandKind::SetAmbientLevel { level: 15 },
    ))
    .await
    .unwrap();
    assert_eq!(
        written(&mut link).await,
        sony_frame(1, &[0x68, 0x17, 0x01, 0x01, 0x01, 0x01, 0x0f])
    );
}

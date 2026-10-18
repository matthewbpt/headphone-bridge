#![cfg(test)]

mod support;

use headphone_bridge_core::frame_log::FrameRecord;
use headphone_bridge_core::platform::Observation;
use headphone_bridge_core::{Core, DeviceState, EffectiveModeReason, NoiseControlMode};
use support::{
    buds4pro, fake_platform, fixture, received, samsung_frame, wait_for, DeviceLink, FakeOs,
};

/// The frames with message id `id` among those the Device sent in `records`.
fn received_with_id(records: &[FrameRecord], id: u8) -> Vec<Vec<u8>> {
    received(records)
        .into_iter()
        .filter(|frame| frame[3] == id)
        .collect()
}

/// Starts a core with a Buds4 Pro Attached through capture-1c's first extended
/// status: R in ear, L in the case, Ambient.
async fn attached_buds4pro() -> (Core, FakeOs, DeviceLink) {
    let (platform, mut os) = fake_platform();
    let core = Core::builder().start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    let mut link = os.next_link().await;
    link.written.recv().await.expect("MANAGER_INFO");
    let status = &received_with_id(&fixture("galaxy-buds4-pro/capture-1c"), 0x61)[0];
    link.send.send(status.clone()).unwrap();
    wait_for(&core, |list| {
        list.devices[0].state == DeviceState::Attached
            && list.devices[0].chosen_mode == Some(NoiseControlMode::Ambient)
    })
    .await;
    (core, os, link)
}

#[tokio::test]
async fn taking_the_buds_out_pauses_the_chosen_mode_until_they_go_back_in() {
    let (core, _os, link) = attached_buds4pro().await;
    let updates = received_with_id(&fixture("galaxy-buds4-pro/capture-1c"), 0x77);

    // capture-1c: `119: 00 22 … 02 …`, both buds out of the ear (cause 02),
    // and the Device turns Ambient off.
    link.send.send(updates[5].clone()).unwrap();
    let list = wait_for(&core, |list| {
        list.devices[0].effective_mode == Some(NoiseControlMode::Off)
    })
    .await;
    assert_eq!(list.devices[0].chosen_mode, Some(NoiseControlMode::Ambient));
    assert_eq!(
        list.devices[0].effective_mode_reason,
        Some(EffectiveModeReason::BudOutOfEar)
    );

    // `119: 02 11 … 02 …`: both back in, and Ambient resumes.
    link.send.send(updates[6].clone()).unwrap();
    let list = wait_for(&core, |list| {
        list.devices[0].effective_mode == Some(NoiseControlMode::Ambient)
    })
    .await;
    assert_eq!(list.devices[0].chosen_mode, Some(NoiseControlMode::Ambient));
    assert_eq!(list.devices[0].effective_mode_reason, None);
}

#[tokio::test]
async fn an_extended_status_while_paused_shows_the_pause_and_not_a_new_chosen_mode() {
    let (core, _os, link) = attached_buds4pro().await;
    let statuses = received_with_id(&fixture("galaxy-buds4-pro/capture-1c"), 0x61);

    // capture-1c: both buds out of the ear (placement 22), and byte 12 reads
    // Off although Ambient is still the Chosen Mode.
    link.send.send(statuses[5].clone()).unwrap();
    let list = wait_for(&core, |list| {
        list.devices[0].effective_mode == Some(NoiseControlMode::Off)
    })
    .await;
    assert_eq!(list.devices[0].chosen_mode, Some(NoiseControlMode::Ambient));
    assert_eq!(
        list.devices[0].effective_mode_reason,
        Some(EffectiveModeReason::BudOutOfEar)
    );

    // Both back in (placement 11), reading Ambient again.
    link.send.send(statuses[7].clone()).unwrap();
    let list = wait_for(&core, |list| {
        list.devices[0].effective_mode == Some(NoiseControlMode::Ambient)
    })
    .await;
    assert_eq!(list.devices[0].chosen_mode, Some(NoiseControlMode::Ambient));
    assert_eq!(list.devices[0].effective_mode_reason, None);
}

#[tokio::test]
async fn a_gesture_while_a_bud_is_out_changes_the_chosen_mode() {
    let (core, _os, link) = attached_buds4pro().await;

    // The listener chooses Noise Cancelling with a pinch-and-hold (capture-4).
    link.send
        .send(samsung_frame(
            0x77,
            &[
                0x01, 0x11, 0x01, 0x00, 0x09, 0x09, 0x01, 0x00, 0x01, 0x02, 0x02,
            ],
        ))
        .unwrap();
    // Then takes the right bud out (placement 12), and the Device turns
    // Noise Cancelling off.
    link.send
        .send(samsung_frame(
            0x77,
            &[
                0x00, 0x12, 0x01, 0x00, 0x09, 0x09, 0x02, 0x02, 0x02, 0x02, 0x02,
            ],
        ))
        .unwrap();
    let list = wait_for(&core, |list| {
        list.devices[0].effective_mode == Some(NoiseControlMode::Off)
    })
    .await;
    assert_eq!(
        list.devices[0].chosen_mode,
        Some(NoiseControlMode::NoiseCancelling)
    );
    assert_eq!(
        list.devices[0].effective_mode_reason,
        Some(EffectiveModeReason::BudOutOfEar)
    );

    // A pinch-and-hold on the bud still in chooses Ambient (cause 01).
    link.send
        .send(samsung_frame(
            0x77,
            &[
                0x02, 0x12, 0x01, 0x00, 0x09, 0x09, 0x01, 0x00, 0x02, 0x02, 0x02,
            ],
        ))
        .unwrap();
    let list = wait_for(&core, |list| {
        list.devices[0].chosen_mode == Some(NoiseControlMode::Ambient)
    })
    .await;
    assert_eq!(
        list.devices[0].effective_mode,
        Some(NoiseControlMode::Ambient)
    );
    assert_eq!(list.devices[0].effective_mode_reason, None);
}

mod support;

use headphone_bridge_core::platform::Observation;
use headphone_bridge_core::{Core, DeviceState};
use support::{buds4pro, fake_platform, fixture, received, samsung_frame, wait_for};

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

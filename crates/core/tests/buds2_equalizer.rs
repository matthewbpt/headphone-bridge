//! The Galaxy Buds2's Equalizer: its six Presets, from GalaxyBudsClient,
//! Gadgetbridge and Samsung's support pages. No real capture has shown a
//! Buds2 off Normal yet, or one ACKing an `EQUALIZER`.

mod support;

use headphone_bridge_core::platform::Observation;
use headphone_bridge_core::{
    Battery, Command, CommandError, Core, DeviceState, Equalizer, EqualizerCapability, Preset,
    PresetId,
};
use support::{
    buds2, fake_platform, fixture, received, replay, samsung_frame, wait_for, DeviceLink, FakeOs,
};

/// Starts a core with a Buds2 Attached through `galaxy-buds2/attach`, the
/// extended status from GalaxyBudsClient's tests, which reports Preset 3,
/// Dynamic (byte 9).
async fn attached_buds2() -> (Core, FakeOs, DeviceLink) {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(buds2()));
    let mut link = os.next_link().await;
    link.written.recv().await.expect("MANAGER_INFO");
    link.send
        .send(received(&fixture("galaxy-buds2/attach"))[0].clone())
        .unwrap();
    wait_for(&core, |list| list.devices[0].state == DeviceState::Attached).await;
    (core, os, link)
}

fn preset(preset: u8) -> Equalizer {
    Equalizer {
        preset: PresetId(preset),
        bands: None,
        clear_bass: None,
    }
}

#[tokio::test]
async fn the_snapshot_carries_the_buds2s_six_presets_and_the_reported_one() {
    let (core, _os, _link) = attached_buds2().await;

    let list = core.device_list();

    assert_eq!(list.devices[0].equalizer, Some(preset(3)));
    let presets = [
        (0, "Normal"),
        (1, "Bass boost"),
        (2, "Soft"),
        (3, "Dynamic"),
        (4, "Clear"),
        (5, "Treble boost"),
    ];
    assert_eq!(
        list.devices[0].capabilities.equalizer,
        Some(EqualizerCapability {
            presets: presets
                .into_iter()
                .map(|(id, name)| Preset {
                    id: PresetId(id),
                    name,
                    editable: false,
                    selectable: true,
                })
                .collect(),
            bands: None,
            clear_bass: None,
        })
    );
}

#[tokio::test]
async fn a_real_buds2_attach_reports_normal() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(buds2()));
    let mut link = os.next_link().await;

    replay(&mut link, &fixture("galaxy-buds2/real-attach")).await;

    let list = wait_for(&core, |list| list.devices[0].equalizer.is_some()).await;
    assert_eq!(list.devices[0].equalizer, Some(preset(0)));
}

/// `galaxy-buds2/attach`'s extended status with byte 9 (the Preset) set to
/// `preset`.
fn extended_status_with_preset(preset: u8) -> Vec<u8> {
    let frame = &received(&fixture("galaxy-buds2/attach"))[0];
    let mut status = frame[4..frame.len() - 3].to_vec();
    status[9] = preset;
    samsung_frame(0x61, &status)
}

#[tokio::test]
async fn a_preset_reported_in_a_later_extended_status_replaces_the_last() {
    // Whether a Buds2 sends one when Samsung's app changes the Preset is
    // unproven.
    let (core, _os, link) = attached_buds2().await;

    link.send.send(extended_status_with_preset(1)).unwrap();

    wait_for(&core, |list| list.devices[0].equalizer == Some(preset(1))).await;
}

#[tokio::test]
async fn a_preset_the_buds2_does_not_list_is_reported_unchanged() {
    let (core, _os, link) = attached_buds2().await;

    link.send.send(extended_status_with_preset(7)).unwrap();

    wait_for(&core, |list| list.devices[0].equalizer == Some(preset(7))).await;
}

/// `EQUALIZER` (0x86) with Preset 1, Bass boost.
const SET_PRESET_1: [u8; 8] = [0xfd, 0x04, 0x00, 0x86, 0x01, 0x1f, 0xa1, 0xdd];
const MANAGER_INFO: [u8; 10] = [0xfd, 0x06, 0x00, 0x88, 0x01, 0x01, 0x22, 0xda, 0x58, 0xdd];

/// Waits for a status that changes the left bud's battery, which shows every
/// frame sent before it has been handled.
async fn handled(core: &Core, link: &DeviceLink) -> Vec<headphone_bridge_core::Device> {
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
    .devices
}

#[tokio::test]
async fn setting_a_preset_sends_equalizer_and_an_ack_carrying_it_confirms_it() {
    let (core, _os, mut link) = attached_buds2().await;

    core.command(Command::SetEqualizer(buds2().address, preset(1)))
        .await
        .unwrap();

    assert_eq!(link.written.recv().await.unwrap(), SET_PRESET_1);
    assert_eq!(link.written.recv().await.unwrap(), MANAGER_INFO);
    // A Buds2 answers MANAGER_INFO with a bare `88 00` after a set
    // (real-noise-control), so the ACK is the only confirmation. One with
    // another value is only a receipt.
    link.send.send(samsung_frame(0x88, &[0x00])).unwrap();
    link.send.send(samsung_frame(0x42, &[0x86, 0x02])).unwrap();
    assert_eq!(handled(&core, &link).await[0].equalizer, Some(preset(3)));

    link.send.send(samsung_frame(0x42, &[0x86, 0x01])).unwrap();

    wait_for(&core, |list| list.devices[0].equalizer == Some(preset(1))).await;
}

/// Sends `equalizer` to the Attached Buds2, and checks it is rejected with
/// `error` and nothing more is written.
async fn rejected(equalizer: Equalizer, error: CommandError) {
    let (core, _os, mut link) = attached_buds2().await;

    let result = core
        .command(Command::SetEqualizer(buds2().address, equalizer))
        .await;

    assert_eq!(result, Err(error));
    drop(core);
    assert_eq!(link.written.recv().await, None, "nothing else written");
}

#[tokio::test]
async fn custom_is_rejected_as_the_buds2_has_no_custom_eq() {
    rejected(preset(7), CommandError::UnavailablePreset).await;
}

#[tokio::test]
async fn a_preset_beyond_treble_boost_is_rejected() {
    rejected(preset(6), CommandError::UnavailablePreset).await;
}

#[tokio::test]
async fn bands_are_rejected_as_the_buds2_has_none() {
    let equalizer = Equalizer {
        bands: Some(vec![0, 0, 0, 0, 0]),
        ..preset(1)
    };
    rejected(equalizer, CommandError::MissingCapability).await;
}

#[tokio::test]
async fn clear_bass_is_rejected_as_the_buds2_has_none() {
    let equalizer = Equalizer {
        clear_bass: Some(0),
        ..preset(1)
    };
    rejected(equalizer, CommandError::MissingCapability).await;
}

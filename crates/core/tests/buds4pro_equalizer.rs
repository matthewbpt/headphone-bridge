mod support;

use headphone_bridge_core::platform::Observation;
use headphone_bridge_core::{
    Battery, Command, CommandError, Core, DeviceState, Equalizer, EqualizerCapability, Preset,
    PresetId,
};
use support::{
    buds4pro, fake_platform, fixture, received, samsung_frame, wait_for, DeviceLink, FakeOs,
};

/// Starts a core with a Buds4 Pro Attached through capture-3's first extended
/// status, which reports Preset 3, Dynamic (byte 9).
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

fn preset(preset: u8) -> Equalizer {
    Equalizer {
        preset: PresetId(preset),
        bands: None,
        clear_bass: None,
    }
}

#[tokio::test]
async fn a_buds4pro_is_attached_with_its_preset_so_a_set_never_finds_it_unreported() {
    // The extended status that identifies the Model also reports the Preset,
    // so the "not reported yet" rejection the XM5 tests can't arise here.
    let (core, _os, _link) = attached_buds4pro().await;

    assert_eq!(core.device_list().devices[0].equalizer, Some(preset(3)));
}

#[tokio::test]
async fn the_snapshot_carries_the_models_presets_and_the_reported_one() {
    let (core, _os, _link) = attached_buds4pro().await;

    let list = wait_for(&core, |list| list.devices[0].equalizer.is_some()).await;

    assert_eq!(list.devices[0].equalizer, Some(preset(3)));
    let presets = [
        (0, "Normal", true),
        (1, "Bass boost", true),
        (2, "Soft", true),
        (3, "Dynamic", true),
        (4, "Clear", true),
        (5, "Treble boost", true),
        (7, "Custom", false),
    ];
    assert_eq!(
        list.devices[0].capabilities.equalizer,
        Some(EqualizerCapability {
            presets: presets
                .into_iter()
                .map(|(id, name, selectable)| Preset {
                    id: PresetId(id),
                    name,
                    editable: false,
                    selectable,
                })
                .collect(),
            bands: None,
            clear_bass: None,
        })
    );
}

#[tokio::test]
async fn the_preset_is_dropped_when_the_device_is_detached() {
    let (core, _os, link) = attached_buds4pro().await;
    wait_for(&core, |list| list.devices[0].equalizer.is_some()).await;

    drop(link);

    let list = wait_for(&core, |list| list.devices[0].state != DeviceState::Attached).await;
    assert_eq!(list.devices[0].equalizer, None, "no stale values remain");
}

/// capture-3's first extended status with byte 9 (the Preset) set to `preset`.
fn extended_status_with_preset(preset: u8) -> Vec<u8> {
    let mut status = received(&fixture("galaxy-buds4-pro/capture-3"))[0][4..66].to_vec();
    status[9] = preset;
    samsung_frame(0x61, &status)
}

#[tokio::test]
async fn a_preset_changed_from_samsungs_app_appears_live() {
    let (core, _os, link) = attached_buds4pro().await;
    wait_for(&core, |list| list.devices[0].equalizer == Some(preset(3))).await;

    link.send.send(extended_status_with_preset(1)).unwrap();

    wait_for(&core, |list| list.devices[0].equalizer == Some(preset(1))).await;
}

#[tokio::test]
async fn a_preset_the_model_does_not_list_is_reported_unchanged() {
    let (core, _os, link) = attached_buds4pro().await;
    wait_for(&core, |list| list.devices[0].equalizer == Some(preset(3))).await;

    link.send.send(extended_status_with_preset(9)).unwrap();

    wait_for(&core, |list| list.devices[0].equalizer == Some(preset(9))).await;
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
    let (core, _os, mut link) = attached_buds4pro().await;
    wait_for(&core, |list| list.devices[0].equalizer == Some(preset(3))).await;

    core.command(Command::SetEqualizer(buds4pro().address, preset(1)))
        .await
        .unwrap();

    assert_eq!(link.written.recv().await.unwrap(), SET_PRESET_1);
    // Like the other sets, polled with MANAGER_INFO for a fresh extended
    // status.
    assert_eq!(link.written.recv().await.unwrap(), MANAGER_INFO);
    // An ACK with another value, such as one for an earlier set, is only a
    // receipt.
    link.send.send(samsung_frame(0x42, &[0x86, 0x02])).unwrap();
    assert_eq!(handled(&core, &link).await[0].equalizer, Some(preset(3)));

    link.send.send(samsung_frame(0x42, &[0x86, 0x01])).unwrap();

    wait_for(&core, |list| list.devices[0].equalizer == Some(preset(1))).await;
}

/// Sends `equalizer` to the Attached Buds4 Pro, and checks it is rejected
/// with `error` and nothing more is written.
async fn rejected(equalizer: Equalizer, error: CommandError) {
    let (core, _os, mut link) = attached_buds4pro().await;
    wait_for(&core, |list| list.devices[0].equalizer.is_some()).await;

    let result = core
        .command(Command::SetEqualizer(buds4pro().address, equalizer))
        .await;

    assert_eq!(result, Err(error));
    drop(core);
    assert_eq!(link.written.recv().await, None, "nothing else written");
}

#[tokio::test]
async fn a_preset_the_model_does_not_list_is_rejected_before_anything_is_sent() {
    rejected(preset(6), CommandError::UnavailablePreset).await;
}

#[tokio::test]
async fn custom_is_rejected_as_only_samsungs_app_can_pick_it() {
    rejected(preset(7), CommandError::UnavailablePreset).await;
}

#[tokio::test]
async fn bands_are_rejected_as_the_model_has_none() {
    let equalizer = Equalizer {
        bands: Some(vec![0, 0, 0, 0, 0]),
        ..preset(1)
    };
    rejected(equalizer, CommandError::MissingCapability).await;
}

#[tokio::test]
async fn clear_bass_is_rejected_as_the_model_has_none() {
    let equalizer = Equalizer {
        clear_bass: Some(0),
        ..preset(1)
    };
    rejected(equalizer, CommandError::MissingCapability).await;
}

#[tokio::test]
async fn a_preset_is_not_sent_before_the_device_is_attached() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    let mut link = os.next_link().await;
    link.written.recv().await.expect("MANAGER_INFO");

    let result = core
        .command(Command::SetEqualizer(buds4pro().address, preset(1)))
        .await;

    assert_eq!(result, Err(CommandError::NotAttached));
    drop(core);
    assert_eq!(link.written.recv().await, None, "nothing else written");
}

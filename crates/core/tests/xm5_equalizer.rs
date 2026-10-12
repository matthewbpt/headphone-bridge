mod support;

use std::time::Duration;

use headphone_bridge_core::platform::Observation;
use headphone_bridge_core::{
    Battery, Command, CommandError, Core, Equalizer, EqualizerCapability, Preset,
};
use support::{
    fake_platform, fixture, replay, sony_ack, sony_frame, wait_for, xm5, DeviceLink, FakeOs,
};

/// Starts a core with a WH-1000XM5 Attached through the whole attach
/// fixture, which reports Bright with Clear Bass +2 and Bands −1, 0, +3, +5
/// and +10.
async fn attached_xm5() -> (Core, FakeOs, DeviceLink) {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    let mut link = os.next_link().await;
    replay(&mut link, &fixture("wh-1000xm5/attach")).await;
    wait_for(&core, |list| list.devices[0].ambient_level.is_some()).await;
    (core, os, link)
}

#[tokio::test]
async fn the_capabilities_list_sonys_presets_and_only_manual_and_the_customs_are_editable() {
    let (core, _os, _link) = attached_xm5().await;

    let presets = [
        (0x00, "Off", false),
        (0x10, "Bright", false),
        (0x11, "Excited", false),
        (0x12, "Mellow", false),
        (0x13, "Relaxed", false),
        (0x14, "Vocal", false),
        (0x15, "Treble Boost", false),
        (0x16, "Bass Boost", false),
        (0x17, "Speech", false),
        (0xa0, "Manual", true),
        (0xa1, "Custom 1", true),
        (0xa2, "Custom 2", true),
    ];
    assert_eq!(
        core.device_list().devices[0].capabilities.equalizer,
        Some(EqualizerCapability {
            presets: presets
                .into_iter()
                .map(|(id, name, editable)| Preset {
                    id,
                    name,
                    editable,
                    selectable: true,
                })
                .collect(),
            bands: None,
            clear_bass: None,
        })
    );
}

#[tokio::test]
async fn attaching_reads_the_preset_and_keeps_the_reported_bands_and_clear_bass() {
    let (core, _os, _link) = attached_xm5().await;

    assert_eq!(core.device_list().devices[0].equalizer, Some(bright()));
}

/// Bright as the attach fixture reports it.
fn bright() -> Equalizer {
    Equalizer {
        preset: 0x10,
        bands: Some(vec![-1, 0, 3, 5, 10]),
        clear_bass: Some(2),
    }
}

fn preset(preset: u8) -> Equalizer {
    Equalizer {
        preset,
        bands: None,
        clear_bass: None,
    }
}

/// The next frame the core writes.
async fn written(link: &mut DeviceLink) -> Vec<u8> {
    tokio::time::timeout(Duration::from_secs(5), link.written.recv())
        .await
        .expect("timed out waiting for the core to write")
        .expect("link closed")
}

#[tokio::test]
async fn setting_a_preset_sends_it_alone_and_waits_for_the_device() {
    let (core, _os, mut link) = attached_xm5().await;

    core.command(Command::SetEqualizer(xm5().address, preset(0x16)))
        .await
        .unwrap();

    // EQEBB_SET_PARAM: Bass Boost, no steps.
    assert_eq!(
        written(&mut link).await,
        sony_frame(1, &[0x58, 0x00, 0x16, 0x00])
    );
    // The ACK is only a receipt. A battery notification after it shows it
    // has been handled.
    link.send.send(sony_ack(0)).unwrap();
    link.send
        .send(sony_frame(0, &[0x25, 0x00, 68, 0x00]))
        .unwrap();
    let list = wait_for(&core, |list| {
        list.devices[0].battery == Some(Battery::Headphones { level: 68 })
    })
    .await;
    assert_eq!(list.devices[0].equalizer, Some(bright()));

    // The Device confirms with Bass Boost's curve.
    link.send
        .send(sony_frame(
            1,
            &[0x59, 0x00, 0x16, 0x06, 0x0a, 0x10, 0x0d, 0x0a, 0x0a, 0x0a],
        ))
        .unwrap();
    wait_for(&core, |list| {
        list.devices[0].equalizer
            == Some(Equalizer {
                preset: 0x16,
                bands: Some(vec![6, 3, 0, 0, 0]),
                clear_bass: Some(0),
            })
    })
    .await;
}

#[tokio::test]
async fn a_preset_set_waits_for_the_last_request_to_be_acked() {
    let (core, _os, mut link) = attached_xm5().await;

    core.command(Command::SetAmbientLevel(xm5().address, 3))
        .await
        .unwrap();
    core.command(Command::SetEqualizer(xm5().address, preset(0xa0)))
        .await
        .unwrap();

    assert_eq!(
        written(&mut link).await,
        sony_frame(1, &[0x68, 0x17, 0x01, 0x01, 0x01, 0x01, 0x03])
    );
    link.send.send(sony_ack(0)).unwrap();
    assert_eq!(
        written(&mut link).await,
        sony_frame(0, &[0x58, 0x00, 0xa0, 0x00])
    );
}

#[tokio::test]
async fn changes_from_sonys_app_appear_live() {
    let (core, _os, mut link) = attached_xm5().await;

    // Sony's app switches to Custom 1, with its curve.
    link.send
        .send(sony_frame(
            0,
            &[0x59, 0x00, 0xa1, 0x06, 0x0b, 0x00, 0x14, 0x0a, 0x08, 0x0c],
        ))
        .unwrap();
    wait_for(&core, |list| {
        list.devices[0].equalizer
            == Some(Equalizer {
                preset: 0xa1,
                bands: Some(vec![-10, 10, 0, -2, 2]),
                clear_bass: Some(1),
            })
    })
    .await;
    // Then to a Preset the app doesn't know, without a curve.
    link.send
        .send(sony_frame(1, &[0x59, 0x00, 0x30, 0x00]))
        .unwrap();
    wait_for(&core, |list| {
        list.devices[0].equalizer == Some(preset(0x30))
    })
    .await;
    // Both were ACKed.
    for _ in 0..2 {
        assert_eq!(written(&mut link).await[1], 0x01, "an ACK");
    }
}

/// Sends `equalizer` to the Attached XM5, and checks it is rejected with
/// `error` and nothing more is written.
async fn rejected(equalizer: Equalizer, error: CommandError) {
    let (core, _os, mut link) = attached_xm5().await;

    let result = core
        .command(Command::SetEqualizer(xm5().address, equalizer))
        .await;

    assert_eq!(result, Err(error));
    drop(core);
    assert_eq!(link.written.recv().await, None, "nothing else written");
}

#[tokio::test]
async fn a_preset_the_model_does_not_list_is_rejected_before_anything_is_sent() {
    rejected(preset(0x18), CommandError::UnavailablePreset).await;
}

#[tokio::test]
async fn bands_and_clear_bass_are_rejected_until_the_app_can_set_them() {
    let bands = Equalizer {
        bands: Some(vec![0, 0, 0, 0, 0]),
        ..preset(0xa0)
    };
    rejected(bands, CommandError::MissingCapability).await;
    let clear_bass = Equalizer {
        clear_bass: Some(0),
        ..preset(0xa0)
    };
    rejected(clear_bass, CommandError::MissingCapability).await;
}

#[tokio::test]
async fn a_preset_set_before_the_device_reports_its_equalizer_is_rejected() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    let mut link = os.next_link().await;
    // Up to the app's ACK of the NCASM reply, before the Equalizer GET.
    let records = fixture("wh-1000xm5/attach");
    replay(&mut link, &records[..20]).await;
    wait_for(&core, |list| list.devices[0].ambient_level.is_some()).await;

    assert_eq!(
        core.command(Command::SetEqualizer(xm5().address, preset(0x16)))
            .await,
        Err(CommandError::SettingsNotReported)
    );

    // Nothing was sent: the rest of the Attach plays out as recorded, and
    // the first set after it is the next thing written.
    replay(&mut link, &records[20..]).await;
    wait_for(&core, |list| list.devices[0].equalizer.is_some()).await;
    core.command(Command::SetEqualizer(xm5().address, preset(0x16)))
        .await
        .unwrap();
    assert_eq!(
        written(&mut link).await,
        sony_frame(1, &[0x58, 0x00, 0x16, 0x00])
    );
}

#[tokio::test]
async fn attaching_reads_whether_the_equalizer_is_unavailable() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    let mut link = os.next_link().await;
    // Up to the app's ACK of the NCASM reply.
    replay(&mut link, &fixture("wh-1000xm5/attach")[..20]).await;

    // EQEBB_GET_STATUS, answered DISABLE.
    assert_eq!(written(&mut link).await, sony_frame(1, &[0x52, 0x00]));
    link.send.send(sony_ack(0)).unwrap();
    link.send.send(sony_frame(1, &[0x53, 0x00, 0x01])).unwrap();
    assert_eq!(written(&mut link).await, sony_ack(0));
    // Then the Equalizer itself.
    assert_eq!(written(&mut link).await, sony_frame(0, &[0x56, 0x00]));

    let list = wait_for(&core, |list| list.devices[0].equalizer_unavailable).await;
    assert_eq!(list.devices[0].equalizer, None);
}

#[tokio::test]
async fn the_attach_fixture_reports_the_equalizer_available() {
    let (core, _os, _link) = attached_xm5().await;

    assert!(!core.device_list().devices[0].equalizer_unavailable);
}

#[tokio::test]
async fn the_device_reporting_its_equalizer_disabled_and_enabled_toggles_it_live() {
    let (core, _os, mut link) = attached_xm5().await;

    // EQEBB_NTFY_STATUS: DISABLE, e.g. while another sound feature is on.
    link.send.send(sony_frame(0, &[0x55, 0x00, 0x01])).unwrap();
    wait_for(&core, |list| list.devices[0].equalizer_unavailable).await;
    // Then ENABLE.
    link.send.send(sony_frame(1, &[0x55, 0x00, 0x00])).unwrap();
    let list = wait_for(&core, |list| !list.devices[0].equalizer_unavailable).await;
    // The Equalizer is kept throughout.
    assert_eq!(list.devices[0].equalizer, Some(bright()));
    for _ in 0..2 {
        assert_eq!(written(&mut link).await[1], 0x01, "an ACK");
    }
}

#[tokio::test]
async fn a_set_while_the_equalizer_is_unavailable_is_rejected_and_nothing_is_sent() {
    let (core, _os, mut link) = attached_xm5().await;
    link.send.send(sony_frame(0, &[0x55, 0x00, 0x01])).unwrap();
    wait_for(&core, |list| list.devices[0].equalizer_unavailable).await;
    assert_eq!(written(&mut link).await[1], 0x01, "an ACK");

    let result = core
        .command(Command::SetEqualizer(xm5().address, preset(0x16)))
        .await;

    assert_eq!(result, Err(CommandError::EqualizerUnavailable));
    drop(core);
    assert_eq!(link.written.recv().await, None, "nothing else written");
}

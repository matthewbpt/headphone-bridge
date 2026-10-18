#![cfg(test)]

mod support;

use std::time::Duration;

use headphone_bridge_core::frame_log::{Direction, FrameRecord};
use headphone_bridge_core::platform::Observation;
use headphone_bridge_core::wire::sony::PRESET_EQ;
use headphone_bridge_core::{
    Bands, Battery, CommandError, CommandKind, Core, Equalizer, EqualizerCapability,
    NoiseControlMode, Preset, PresetId, StepRange,
};
use support::{
    device_command, fake_platform, fixture, linked_core, replay, sony_ack, sony_decode, sony_frame,
    wait_for, without_capability_probe, xm5, DeviceLink, FakeOs, EQEBB_RET_CAPABILITY,
};

/// Starts a core with a WH-1000XM5 Attached through the whole attach
/// fixture, which reports Bright with Clear Bass +2 and Bands −1, 0, +3, +5
/// and +10.
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
async fn the_capabilities_list_sonys_presets_five_bands_and_clear_bass_and_only_manual_and_the_customs_are_editable(
) {
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
                    id: PresetId(id),
                    name,
                    editable,
                    selectable: true,
                })
                .collect(),
            bands: Some(Bands {
                frequencies: vec!["400 Hz", "1 kHz", "2.5 kHz", "6.3 kHz", "16 kHz"],
                range: StepRange { min: -10, max: 10 },
            }),
            clear_bass: Some(StepRange { min: -10, max: 10 }),
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
        preset: PresetId(0x10),
        bands: Some(vec![-1, 0, 3, 5, 10]),
        clear_bass: Some(2),
    }
}

fn preset(preset: u8) -> Equalizer {
    Equalizer {
        preset: PresetId(preset),
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

    core.command(device_command(
        xm5().address,
        CommandKind::SetEqualizer {
            equalizer: preset(0x16),
        },
    ))
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
                preset: PresetId(0x16),
                bands: Some(vec![6, 3, 0, 0, 0]),
                clear_bass: Some(0),
            })
    })
    .await;
}

#[tokio::test]
async fn moving_from_a_fixed_preset_to_manual_sends_the_whole_curve_and_waits_for_the_device() {
    let (core, _os, mut link) = attached_xm5().await;
    // Bright's curve with the 2.5 kHz Band raised to +6, as the UI sends it.
    let manual = Equalizer {
        preset: PresetId(0xa0),
        bands: Some(vec![-1, 0, 6, 5, 10]),
        clear_bass: Some(2),
    };

    core.command(device_command(
        xm5().address,
        CommandKind::SetEqualizer { equalizer: manual },
    ))
    .await
    .unwrap();

    // EQEBB_SET_PARAM: Manual, six steps each + 10: Bright's Clear Bass
    // (+2), then the Bands.
    assert_eq!(
        written(&mut link).await,
        sony_frame(
            1,
            &[0x58, 0x00, 0xa0, 0x06, 0x0c, 0x09, 0x0a, 0x10, 0x0f, 0x14]
        )
    );
    link.send.send(sony_ack(0)).unwrap();
    // The Device confirms.
    link.send
        .send(sony_frame(
            0,
            &[0x59, 0x00, 0xa0, 0x06, 0x0c, 0x09, 0x0a, 0x10, 0x0f, 0x14],
        ))
        .unwrap();
    wait_for(&core, |list| {
        list.devices[0].equalizer
            == Some(Equalizer {
                preset: PresetId(0xa0),
                bands: Some(vec![-1, 0, 6, 5, 10]),
                clear_bass: Some(2),
            })
    })
    .await;
}

#[tokio::test]
async fn editing_a_band_on_custom_1_stays_on_custom_1() {
    let (core, _os, mut link) = attached_xm5().await;
    // The Device notifies a switch to Custom 1, with its curve.
    link.send
        .send(sony_frame(
            0,
            &[0x59, 0x00, 0xa1, 0x06, 0x0b, 0x00, 0x14, 0x0a, 0x08, 0x0c],
        ))
        .unwrap();
    wait_for(&core, |list| {
        list.devices[0].equalizer.as_ref().map(|eq| eq.preset) == Some(PresetId(0xa1))
    })
    .await;
    assert_eq!(written(&mut link).await[1], 0x01, "an ACK");

    let custom_1 = Equalizer {
        preset: PresetId(0xa1),
        bands: Some(vec![-10, 10, 0, -2, -10]),
        clear_bass: Some(1),
    };
    core.command(device_command(
        xm5().address,
        CommandKind::SetEqualizer {
            equalizer: custom_1,
        },
    ))
    .await
    .unwrap();

    assert_eq!(
        written(&mut link).await,
        sony_frame(
            1,
            &[0x58, 0x00, 0xa1, 0x06, 0x0b, 0x00, 0x14, 0x0a, 0x08, 0x00]
        )
    );
}

#[tokio::test]
async fn a_preset_set_waits_for_the_last_request_to_be_acked() {
    let (core, _os, mut link) = attached_xm5().await;

    core.command(device_command(
        xm5().address,
        CommandKind::SetAmbientLevel { level: 3 },
    ))
    .await
    .unwrap();
    core.command(device_command(
        xm5().address,
        CommandKind::SetEqualizer {
            equalizer: preset(0xa0),
        },
    ))
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
async fn changes_the_device_notifies_appear_live() {
    let (core, _os, mut link) = attached_xm5().await;

    // The Device notifies a switch to Custom 1, with its curve.
    link.send
        .send(sony_frame(
            0,
            &[0x59, 0x00, 0xa1, 0x06, 0x0b, 0x00, 0x14, 0x0a, 0x08, 0x0c],
        ))
        .unwrap();
    wait_for(&core, |list| {
        list.devices[0].equalizer
            == Some(Equalizer {
                preset: PresetId(0xa1),
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
        .command(device_command(
            xm5().address,
            CommandKind::SetEqualizer { equalizer },
        ))
        .await;

    assert_eq!(result, Err(error));
    drop(core);
    assert_eq!(link.written.recv().await, None, "nothing else written");
}

#[tokio::test]
async fn a_preset_the_model_does_not_list_is_rejected_before_anything_is_sent() {
    rejected(preset(0x18), CommandError::UnavailablePreset).await;
}

/// A curve at `preset`: `bands`, with Clear Bass at 0.
fn bands(preset: u8, bands: Vec<i8>) -> Equalizer {
    Equalizer {
        bands: Some(bands),
        clear_bass: Some(0),
        ..self::preset(preset)
    }
}

#[tokio::test]
async fn bands_for_a_fixed_preset_are_rejected_before_anything_is_sent() {
    rejected(bands(0x10, vec![0, 0, 0, 0, 0]), CommandError::FixedPreset).await;
}

#[tokio::test]
async fn bands_outside_the_range_are_rejected_before_anything_is_sent() {
    rejected(bands(0xa0, vec![0, 0, 11, 0, 0]), CommandError::OutOfRange).await;
    rejected(bands(0xa0, vec![-11, 0, 0, 0, 0]), CommandError::OutOfRange).await;
}

#[tokio::test]
async fn a_curve_without_one_value_per_band_is_rejected_before_anything_is_sent() {
    rejected(bands(0xa0, vec![0, 0, 0, 0]), CommandError::OutOfRange).await;
    rejected(
        bands(0xa0, vec![0, 0, 0, 0, 0, 0]),
        CommandError::OutOfRange,
    )
    .await;
}

/// Starts a core with an Attached WH-1000XM5 that has notified a move
/// onto Manual, with Clear Bass +1 and Bands −10, +10, 0, −2 and +2.
async fn xm5_on_manual() -> (Core, FakeOs, DeviceLink) {
    let (core, os, mut link) = attached_xm5().await;
    link.send
        .send(sony_frame(
            0,
            &[0x59, 0x00, 0xa0, 0x06, 0x0b, 0x00, 0x14, 0x0a, 0x08, 0x0c],
        ))
        .unwrap();
    wait_for(&core, |list| {
        list.devices[0].equalizer.as_ref().map(|eq| eq.preset) == Some(PresetId(0xa0))
    })
    .await;
    assert_eq!(written(&mut link).await[1], 0x01, "an ACK");
    (core, os, link)
}

#[tokio::test]
async fn changing_clear_bass_sends_it_first_in_the_curve_and_waits_for_the_device() {
    let (core, _os, mut link) = xm5_on_manual().await;
    let louder = Equalizer {
        preset: PresetId(0xa0),
        bands: Some(vec![-10, 10, 0, -2, 2]),
        clear_bass: Some(7),
    };

    core.command(device_command(
        xm5().address,
        CommandKind::SetEqualizer {
            equalizer: louder.clone(),
        },
    ))
    .await
    .unwrap();

    // EQEBB_SET_PARAM: Manual, six steps each + 10, Clear Bass first.
    assert_eq!(
        written(&mut link).await,
        sony_frame(
            1,
            &[0x58, 0x00, 0xa0, 0x06, 0x11, 0x00, 0x14, 0x0a, 0x08, 0x0c]
        )
    );
    link.send.send(sony_ack(0)).unwrap();
    link.send
        .send(sony_frame(
            1,
            &[0x59, 0x00, 0xa0, 0x06, 0x11, 0x00, 0x14, 0x0a, 0x08, 0x0c],
        ))
        .unwrap();
    wait_for(&core, |list| {
        list.devices[0].equalizer == Some(louder.clone())
    })
    .await;
}

#[tokio::test]
async fn a_clear_bass_change_the_device_notifies_appears_live() {
    let (core, _os, mut link) = xm5_on_manual().await;

    // The Device notifies Clear Bass raised to +10, leaving the Bands.
    link.send
        .send(sony_frame(
            1,
            &[0x59, 0x00, 0xa0, 0x06, 0x14, 0x00, 0x14, 0x0a, 0x08, 0x0c],
        ))
        .unwrap();
    wait_for(&core, |list| {
        list.devices[0].equalizer
            == Some(Equalizer {
                preset: PresetId(0xa0),
                bands: Some(vec![-10, 10, 0, -2, 2]),
                clear_bass: Some(10),
            })
    })
    .await;
    assert_eq!(written(&mut link).await[1], 0x01, "an ACK");
}

fn clear_bass(preset: u8, clear_bass: i8) -> Equalizer {
    Equalizer {
        bands: Some(vec![0, 0, 0, 0, 0]),
        clear_bass: Some(clear_bass),
        ..self::preset(preset)
    }
}

#[tokio::test]
async fn clear_bass_for_a_fixed_preset_is_rejected_before_anything_is_sent() {
    rejected(
        Equalizer {
            clear_bass: Some(3),
            ..preset(0x10)
        },
        CommandError::FixedPreset,
    )
    .await;
    rejected(clear_bass(0x16, 3), CommandError::FixedPreset).await;
}

#[tokio::test]
async fn clear_bass_outside_the_range_is_rejected_before_anything_is_sent() {
    rejected(clear_bass(0xa0, 11), CommandError::OutOfRange).await;
    rejected(clear_bass(0xa0, -11), CommandError::OutOfRange).await;
}

#[tokio::test]
async fn a_preset_set_before_the_device_reports_its_equalizer_is_rejected() {
    let (platform, mut os) = fake_platform();
    let core = Core::builder().start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    let mut link = os.next_link().await;
    // Up to the app's ACK of the NCASM reply, before the Equalizer GET.
    let records = fixture("wh-1000xm5/attach");
    replay(&mut link, &records[..20]).await;
    wait_for(&core, |list| list.devices[0].ambient_level.is_some()).await;

    assert_eq!(
        core.command(device_command(
            xm5().address,
            CommandKind::SetEqualizer {
                equalizer: preset(0x16)
            }
        ))
        .await,
        Err(CommandError::SettingsNotReported)
    );

    // Nothing was sent: the rest of the Attach plays out as recorded, and
    // the first set after it is the next thing written.
    replay(&mut link, &records[20..]).await;
    wait_for(&core, |list| list.devices[0].equalizer.is_some()).await;
    core.command(device_command(
        xm5().address,
        CommandKind::SetEqualizer {
            equalizer: preset(0x16),
        },
    ))
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
    let core = Core::builder().start(platform);
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
        .command(device_command(
            xm5().address,
            CommandKind::SetEqualizer {
                equalizer: preset(0x16),
            },
        ))
        .await;
    // Before anything else is checked: even a Preset the Model lacks.
    let unlisted = core
        .command(device_command(
            xm5().address,
            CommandKind::SetEqualizer {
                equalizer: preset(0x18),
            },
        ))
        .await;

    assert_eq!(result, Err(CommandError::EqualizerUnavailable));
    assert_eq!(unlisted, Err(CommandError::EqualizerUnavailable));
    drop(core);
    assert_eq!(link.written.recv().await, None, "nothing else written");
}

#[tokio::test]
async fn a_set_is_rejected_as_unavailable_even_before_the_device_reports_its_equalizer() {
    let (platform, mut os) = fake_platform();
    let core = Core::builder().start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    let mut link = os.next_link().await;
    replay(&mut link, &fixture("wh-1000xm5/attach")[..20]).await;
    // EQEBB_GET_STATUS, answered DISABLE, and the Equalizer GET left
    // unanswered.
    assert_eq!(written(&mut link).await, sony_frame(1, &[0x52, 0x00]));
    link.send.send(sony_ack(0)).unwrap();
    link.send.send(sony_frame(1, &[0x53, 0x00, 0x01])).unwrap();
    assert_eq!(written(&mut link).await, sony_ack(0));
    assert_eq!(written(&mut link).await, sony_frame(0, &[0x56, 0x00]));
    wait_for(&core, |list| list.devices[0].equalizer_unavailable).await;

    let result = core
        .command(device_command(
            xm5().address,
            CommandKind::SetEqualizer {
                equalizer: preset(0x16),
            },
        ))
        .await;

    assert_eq!(result, Err(CommandError::EqualizerUnavailable));
    drop(core);
    assert_eq!(link.written.recv().await, None, "nothing else written");
}

#[tokio::test]
async fn a_curve_edit_the_device_only_acks_is_read_back() {
    // A real WH-1000XM5 already on Manual ACKs a curve edit and sends no
    // notification; only a Preset change brings one.
    let (core, _os, mut link) = xm5_on_manual().await;
    let louder = Equalizer {
        preset: PresetId(0xa0),
        bands: Some(vec![-10, 10, 0, -2, 2]),
        clear_bass: Some(7),
    };

    core.command(device_command(
        xm5().address,
        CommandKind::SetEqualizer {
            equalizer: louder.clone(),
        },
    ))
    .await
    .unwrap();

    assert_eq!(
        written(&mut link).await,
        sony_frame(
            1,
            &[0x58, 0x00, 0xa0, 0x06, 0x11, 0x00, 0x14, 0x0a, 0x08, 0x0c]
        )
    );
    link.send.send(sony_ack(0)).unwrap();
    // EQEBB_GET_PARAM, and the Device's reply settles the edit.
    assert_eq!(written(&mut link).await, sony_frame(0, &[0x56, 0x00]));
    link.send.send(sony_ack(1)).unwrap();
    link.send
        .send(sony_frame(
            1,
            &[0x57, 0x00, 0xa0, 0x06, 0x11, 0x00, 0x14, 0x0a, 0x08, 0x0c],
        ))
        .unwrap();
    wait_for(&core, |list| {
        list.devices[0].equalizer == Some(louder.clone())
    })
    .await;
}

/// The Equalizer a real `EQEBB_SET_PARAM` or notification carries after its
/// command id and type: a Preset, then its curve if it has one.
fn carried(data: &[u8]) -> Equalizer {
    match data[3..] {
        [6, clear_bass, ref bands @ ..] => Equalizer {
            preset: PresetId(data[2]),
            clear_bass: Some(clear_bass as i8 - 10),
            bands: Some(bands.iter().map(|&band| band as i8 - 10).collect()),
        },
        _ => Equalizer {
            preset: PresetId(data[2]),
            bands: None,
            clear_bass: None,
        },
    }
}

/// Attaches a core to the real WH-1000XM5 of real-equalizer-presets and plays
/// its Preset picks, checking each Preset's curve as the Device reports it.
/// It ends on Excited.
async fn real_xm5_after_its_preset_picks() -> (Core, FakeOs, DeviceLink) {
    // Each Preset picked, with the Clear Bass and Bands the Device reported.
    let picks = [
        (0x10, -1, [0, 5, 7, 7, 9]),      // Bright
        (0x11, 8, [-1, 1, 0, 3, 5]),      // Excited
        (0x10, -1, [0, 5, 7, 7, 9]),      // Bright
        (0x13, -9, [-3, -1, -3, -5, -8]), // Relaxed
        (0x16, 7, [0, 0, 0, 0, 0]),       // Bass Boost
        (0xa0, 0, [0, 0, 0, 0, 0]),       // Manual
        (0x17, -10, [4, 3, 1, 2, -10]),   // Speech
        (0x15, 0, [0, 0, 2, 6, 10]),      // Treble Boost
        (0x13, -9, [-3, -1, -3, -5, -8]), // Relaxed
        (0x00, 0, [0, 0, 0, 0, 0]),       // Off
        (0x10, -1, [0, 5, 7, 7, 9]),      // Bright
        (0x11, 8, [-1, 1, 0, 3, 5]),      // Excited
    ];
    replay_preset_picks(fixture("wh-1000xm5/real-equalizer-presets"), &picks).await
}

/// Attaches a core to the real WH-1000XM5 of `records`, which reads the
/// Equalizer as Off and flat, then picks each of `picks` and plays the
/// Device's side from `records`, checking the curve the Device reports.
async fn replay_preset_picks(
    records: Vec<FrameRecord>,
    picks: &[(u8, i8, [i8; 5])],
) -> (Core, FakeOs, DeviceLink) {
    let (core, os, mut link) = linked_core(xm5()).await;
    // The capture split before each EQEBB_SET_PARAM the app sent.
    let set_at: Vec<usize> = records
        .iter()
        .enumerate()
        .filter(|(_, r)| r.direction == Direction::Sent && r.bytes.get(7) == Some(&0x58))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(set_at.len(), picks.len());

    replay(&mut link, &records[..set_at[0]]).await;
    // Attach reads the Equalizer: available, on Off and flat.
    let list = wait_for(&core, |list| list.devices[0].equalizer.is_some()).await;
    assert!(!list.devices[0].equalizer_unavailable);
    assert_eq!(
        list.devices[0].equalizer,
        Some(carried(&[0x57, 0x00, 0x00, 6, 10, 10, 10, 10, 10, 10]))
    );
    for (i, &(preset, clear_bass, bands)) in picks.iter().enumerate() {
        core.command(device_command(
            xm5().address,
            CommandKind::SetEqualizer {
                equalizer: self::preset(preset),
            },
        ))
        .await
        .unwrap();
        let end = set_at.get(i + 1).copied().unwrap_or(records.len());
        replay(&mut link, &records[set_at[i]..end]).await;
        let reported = Equalizer {
            preset: PresetId(preset),
            bands: Some(bands.to_vec()),
            clear_bass: Some(clear_bass),
        };
        wait_for(&core, |list| {
            list.devices[0].equalizer == Some(reported.clone())
        })
        .await;
    }
    (core, os, link)
}

#[tokio::test]
async fn real_xm5_preset_picks_replay_from_their_frame_log() {
    real_xm5_after_its_preset_picks().await;
}

#[tokio::test]
async fn real_xm5_picks_of_mellow_vocal_and_the_customs_replay_from_their_frame_log() {
    let picks = [
        (0x12, -3, [-1, -2, -3, -4, -6]), // Mellow
        (0x14, 0, [6, 4, 2, 3, -1]),      // Vocal
        (0xa1, 0, [0, 0, 0, 0, 0]),       // Custom 1
        (0xa2, 0, [0, 0, 0, 0, 0]),       // Custom 2
    ];
    let records = without_capability_probe(&fixture("wh-1000xm5/real-equalizer-more-presets"));
    replay_preset_picks(records, &picks).await;
}

#[tokio::test]
async fn the_capabilities_list_the_presets_and_steps_a_real_xm5_advertises() {
    let (core, _os, _link) = attached_xm5().await;
    // The EQEBB_RET_CAPABILITY the probe drew: its type, how many settings
    // it steps (Clear Bass and each Band), the step count, then each Preset's
    // id and name, length first.
    let advertised = fixture("wh-1000xm5/real-equalizer-more-presets")
        .iter()
        .map(|r| sony_decode(&r.bytes).2)
        .find(|data| data.first() == Some(&EQEBB_RET_CAPABILITY))
        .unwrap();
    let [EQEBB_RET_CAPABILITY, PRESET_EQ, stepped_settings, steps, count, ref presets @ ..] =
        advertised[..]
    else {
        panic!("not a PRESET_EQ capability: {advertised:02x?}");
    };
    let mut ids = Vec::new();
    let mut names = Vec::new();
    let mut rest = presets;
    while let [id, len, ref tail @ ..] = *rest {
        ids.push(PresetId(id));
        names.push(&tail[..len as usize]);
        rest = &tail[len as usize..];
    }
    assert_eq!(ids.len(), count as usize);
    // The XM5 advertises no names, so the Model's own can't be checked
    // against them. A capture that carries some should check them.
    assert!(names.iter().all(|name| name.is_empty()), "{names:02x?}");

    let list = core.device_list();
    let equalizer = list.devices[0].capabilities.equalizer.as_ref().unwrap();
    let listed: Vec<PresetId> = equalizer.presets.iter().map(|p| p.id).collect();
    assert_eq!(listed, ids);
    let model_bands = equalizer.bands.as_ref().unwrap();
    assert_eq!(1 + model_bands.frequencies.len(), stepped_settings as usize);
    for range in [model_bands.range, equalizer.clear_bass.unwrap()] {
        assert_eq!((range.max - range.min + 1) as u8, steps);
    }
}

/// The next data frame the core writes, past any ACKs.
async fn written_data(link: &mut DeviceLink) -> (u8, Vec<u8>) {
    loop {
        let (data_type, seq, data) = sony_decode(&written(link).await);
        if data_type != 0x01 {
            return (seq, data);
        }
    }
}

#[tokio::test]
async fn real_xm5_curve_edits_are_read_back_after_their_ack() {
    use NoiseControlMode::*;
    let (core, _os, mut link) = real_xm5_after_its_preset_picks().await;
    // Recorded before the app read a curve back, so the Device's side is
    // played from it: each notification as recorded, an ACK for each frame
    // the core sends, and a reply to each read with the curve last set.
    let mut device_seq = 0;
    for record in fixture("wh-1000xm5/real-equalizer-curve") {
        let (data_type, _, data) = sony_decode(&record.bytes);
        if data_type == 0x01 {
            continue;
        }
        if record.direction == Direction::Received {
            link.send.send(record.bytes).unwrap();
            continue;
        }
        // A set the app sent: sent again, with the same data. Besides the
        // Equalizer, the listener moved to Ambient and back mid-capture.
        let asked = carried(&data);
        let kind = match data[..] {
            [0x68, _, _, _, 0x01, ..] => CommandKind::SetNoiseControlMode { mode: Ambient },
            [0x68, ..] => CommandKind::SetNoiseControlMode {
                mode: NoiseCancelling,
            },
            _ => CommandKind::SetEqualizer {
                equalizer: asked.clone(),
            },
        };
        let curve = matches!(kind, CommandKind::SetEqualizer { .. }) && asked.bands.is_some();
        core.command(device_command(xm5().address, kind))
            .await
            .unwrap();
        let (seq, sent) = written_data(&mut link).await;
        assert_eq!(sent, data, "expected {record}");
        link.send.send(sony_ack(1 - seq)).unwrap();
        if !curve {
            continue;
        }
        // EQEBB_GET_PARAM, answered as the Device holds the curve.
        let (seq, read) = written_data(&mut link).await;
        assert_eq!(read, [0x56, 0x00]);
        link.send.send(sony_ack(1 - seq)).unwrap();
        let reply = [&[0x57][..], &data[1..]].concat();
        link.send.send(sony_frame(device_seq, &reply)).unwrap();
        device_seq = 1 - device_seq;
        wait_for(&core, |list| {
            list.devices[0].equalizer == Some(asked.clone())
        })
        .await;
    }
    // The last edit: Clear Bass +3 on Manual, only ACKed by the Device.
    let list = core.device_list();
    assert_eq!(
        list.devices[0].equalizer,
        Some(carried(&[0x58, 0x00, 0xa0, 6, 13, 10, 8, 17, 17, 19]))
    );
}

#[tokio::test]
async fn a_real_xm5_keeps_a_curve_it_only_acked_across_an_attach() {
    let (platform, mut os) = fake_platform();
    let core = Core::builder().start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    let mut link = os.next_link().await;

    replay(&mut link, &fixture("wh-1000xm5/real-equalizer-kept")).await;

    // Manual, Clear Bass +3 and Bands 0, −2, +7, +7, +9: real-equalizer-curve's
    // last edit.
    wait_for(&core, |list| {
        list.devices[0].equalizer
            == Some(Equalizer {
                preset: PresetId(0xa0),
                bands: Some(vec![0, -2, 7, 7, 9]),
                clear_bass: Some(3),
            })
    })
    .await;
}

#[tokio::test]
async fn a_change_made_in_sonys_app_between_two_attaches_is_read_on_the_second() {
    // Sony's app can't Attach while this app is, so the listener changed
    // the Equalizer between two Attaches: Custom 2 and flat before, then
    // Custom 1 with Clear Bass +5.
    let (core, _os, mut link) = linked_core(xm5()).await;

    let records = fixture("wh-1000xm5/real-equalizer-from-sonys-app");
    replay(&mut link, &without_capability_probe(&records)).await;

    wait_for(&core, |list| {
        list.devices[0].equalizer
            == Some(Equalizer {
                preset: PresetId(0xa1),
                bands: Some(vec![0, 0, 0, 0, 0]),
                clear_bass: Some(5),
            })
    })
    .await;
}

#[tokio::test]
async fn bands_or_clear_bass_alone_are_rejected_before_anything_is_sent() {
    // The XM5 takes its Bands and Clear Bass in one message.
    rejected(
        Equalizer {
            bands: Some(vec![0, 0, 0, 0, 0]),
            ..preset(0xa0)
        },
        CommandError::IncompleteEqualizer,
    )
    .await;
    rejected(
        Equalizer {
            clear_bass: Some(3),
            ..preset(0xa0)
        },
        CommandError::IncompleteEqualizer,
    )
    .await;
}

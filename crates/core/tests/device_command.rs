#![cfg(test)]

use headphone_bridge_core::platform::DeviceAddress;
use headphone_bridge_core::{CommandKind, DeviceCommand, Equalizer, NoiseControlMode, PresetId};
use serde_json::json;

fn command(kind: CommandKind) -> DeviceCommand {
    DeviceCommand {
        address: DeviceAddress::parse("AA:BB:CC:DD:EE:FF").unwrap(),
        kind,
    }
}

#[test]
fn each_device_command_round_trips_as_flat_json_tagged_by_kind() {
    let cases = [
        (
            command(CommandKind::SetNoiseControlMode {
                mode: NoiseControlMode::Ambient,
            }),
            json!({"kind": "setNoiseControlMode", "address": "AA:BB:CC:DD:EE:FF",
                   "mode": "ambient"}),
        ),
        (
            command(CommandKind::SetAmbientLevel { level: 3 }),
            json!({"kind": "setAmbientLevel", "address": "AA:BB:CC:DD:EE:FF", "level": 3}),
        ),
        (
            command(CommandKind::SetNoiseCancellingLevel { level: 2 }),
            json!({"kind": "setNoiseCancellingLevel", "address": "AA:BB:CC:DD:EE:FF",
                   "level": 2}),
        ),
        (
            command(CommandKind::SetEqualizer {
                equalizer: Equalizer {
                    preset: PresetId(10),
                    bands: Some(vec![-2, 0, 3, 1, -1]),
                    clear_bass: Some(4),
                },
            }),
            json!({"kind": "setEqualizer", "address": "AA:BB:CC:DD:EE:FF",
                   "equalizer": {"preset": 10, "bands": [-2, 0, 3, 1, -1], "clearBass": 4}}),
        ),
        (
            command(CommandKind::PowerOff),
            json!({"kind": "powerOff", "address": "AA:BB:CC:DD:EE:FF"}),
        ),
        (
            command(CommandKind::RetryAttach),
            json!({"kind": "retryAttach", "address": "AA:BB:CC:DD:EE:FF"}),
        ),
    ];
    for (command, wire) in cases {
        assert_eq!(serde_json::to_value(&command).unwrap(), wire, "{command:?}");
        assert_eq!(
            serde_json::from_value::<DeviceCommand>(wire.clone()).unwrap(),
            command,
            "{wire}"
        );
    }
}

#[test]
fn a_device_command_reads_its_address_through_the_parse() {
    let command: DeviceCommand =
        serde_json::from_value(json!({"kind": "powerOff", "address": "aa-bb-cc-dd-ee-ff"}))
            .unwrap();
    assert_eq!(command.address.to_string(), "AA:BB:CC:DD:EE:FF");
}

#[test]
fn a_device_command_cannot_switch_the_frame_log() {
    for wire in [
        json!({"kind": "setFrameLog", "address": "AA:BB:CC:DD:EE:FF", "path": "/tmp/log"}),
        json!({"kind": "setFrameLog", "path": null}),
    ] {
        assert!(
            serde_json::from_value::<DeviceCommand>(wire.clone()).is_err(),
            "{wire}"
        );
    }
}

#[test]
fn a_device_command_without_a_well_formed_address_is_refused() {
    for wire in [
        json!({"kind": "powerOff"}),
        json!({"kind": "powerOff", "address": "AA:BB:CC:DD:EE"}),
        json!({"kind": "setAmbientLevel", "address": "AA_BB_CC_DD_EE_FF", "level": 3}),
    ] {
        assert!(
            serde_json::from_value::<DeviceCommand>(wire.clone()).is_err(),
            "{wire}"
        );
    }
}

//! Which Bluetooth Device, if any, the default audio sink plays to.

#![cfg(test)]

use headphone_bridge_core::platform::DeviceAddress;
use headphone_bridge_platform_linux::{sink_device, Sink};

fn address(s: &str) -> Option<DeviceAddress> {
    Some(DeviceAddress::parse(s).unwrap())
}

#[test]
fn a_pipewire_bluetooth_sink_names_its_device_in_its_properties() {
    let sink = Sink {
        name: "bluez_output.AC_80_0A_11_22_33.1",
        bluez5_address: Some("ac:80:0a:11:22:33"),
        device_string: None,
    };
    assert_eq!(sink_device(&sink), address("AC:80:0A:11:22:33"));
}

#[test]
fn a_pulseaudio_bluetooth_sink_names_its_device_in_device_string() {
    let sink = Sink {
        name: "bluez_sink.AC_80_0A_11_22_33.a2dp_sink",
        bluez5_address: None,
        device_string: Some("AC:80:0A:11:22:33"),
    };
    assert_eq!(sink_device(&sink), address("AC:80:0A:11:22:33"));
}

#[test]
fn a_bluetooth_sink_without_properties_is_matched_by_its_name() {
    for name in [
        "bluez_output.AC_80_0A_11_22_33.1",
        "bluez_output.AC_80_0A_11_22_33.a2dp-sink",
        "bluez_sink.ac_80_0a_11_22_33.a2dp_sink",
    ] {
        let sink = Sink {
            name,
            bluez5_address: None,
            device_string: None,
        };
        assert_eq!(sink_device(&sink), address("AC:80:0A:11:22:33"), "{name}");
    }
}

#[test]
fn other_sinks_are_not_bluetooth_devices() {
    for (name, device_string) in [
        (
            "alsa_output.pci-0000_00_1f.3.analog-stereo",
            Some("front:0"),
        ),
        ("alsa_output.usb-Focusrite_Scarlett-00.analog-stereo", None),
        ("auto_null", None),
    ] {
        let sink = Sink {
            name,
            bluez5_address: None,
            device_string,
        };
        assert_eq!(sink_device(&sink), None, "{name}");
    }
}

#[test]
fn a_sink_with_a_malformed_bluez5_address_falls_back_to_device_string_then_its_name() {
    let sink = Sink {
        name: "bluez_output.11_22_33_44_55_66.1",
        bluez5_address: Some("not an address"),
        device_string: Some("AC:80:0A:11:22:33"),
    };
    assert_eq!(sink_device(&sink), address("AC:80:0A:11:22:33"));

    let sink = Sink {
        device_string: None,
        ..sink
    };
    assert_eq!(sink_device(&sink), address("11:22:33:44:55:66"));
}

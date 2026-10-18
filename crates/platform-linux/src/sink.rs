//! Matching the default audio sink to a Bluetooth Device. Pure, so it builds
//! and is tested on every OS.

use headphone_bridge_core::platform::DeviceAddress;

/// What PulseAudio (or PipeWire's PulseAudio server) says about a sink.
#[derive(Debug, Clone, Copy)]
pub struct Sink<'a> {
    /// e.g. `bluez_output.AC_80_0A_11_22_33.1` (PipeWire) or
    /// `bluez_sink.AC_80_0A_11_22_33.a2dp_sink` (PulseAudio).
    pub name: &'a str,
    /// The `api.bluez5.address` property, set by PipeWire.
    pub bluez5_address: Option<&'a str>,
    /// The `device.string` property, the address on PulseAudio's Bluetooth sinks.
    pub device_string: Option<&'a str>,
}

/// The Bluetooth Device `sink` plays to, or `None` for any other sink. Each
/// place the address can be is tried in turn, past any that doesn't parse.
pub fn sink_device(sink: &Sink) -> Option<DeviceAddress> {
    let from_name = || {
        let rest = sink
            .name
            .strip_prefix("bluez_output.")
            .or_else(|| sink.name.strip_prefix("bluez_sink."))?;
        rest.get(..17)?.replace('_', ":").parse().ok()
    };
    sink.bluez5_address
        .and_then(|s| s.parse().ok())
        .or_else(|| sink.device_string.and_then(|s| s.parse().ok()))
        .or_else(from_name)
}

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

/// The Bluetooth Device `sink` plays to, or `None` for any other sink.
pub fn sink_device(sink: &Sink) -> Option<DeviceAddress> {
    let from_name = || {
        let rest = sink
            .name
            .strip_prefix("bluez_output.")
            .or_else(|| sink.name.strip_prefix("bluez_sink."))?;
        Some(rest.get(..17)?.replace('_', ":"))
    };
    let address = sink
        .bluez5_address
        .map(str::to_string)
        .or_else(|| {
            sink.device_string
                .filter(|s| is_address(s))
                .map(str::to_string)
        })
        .or_else(from_name)?;
    is_address(&address).then(|| DeviceAddress(address.to_ascii_uppercase()))
}

/// Whether `s` has the form `AA:BB:CC:DD:EE:FF`.
fn is_address(s: &str) -> bool {
    s.len() == 17
        && s.split(':').count() == 6
        && s.split(':')
            .all(|octet| octet.len() == 2 && octet.chars().all(|c| c.is_ascii_hexdigit()))
}

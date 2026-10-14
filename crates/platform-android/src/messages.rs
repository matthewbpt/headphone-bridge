//! The JSON messages between Kotlin and the core, and their mapping to the
//! core's types.

use headphone_bridge_core::platform::{DeviceAddress, DeviceIdentity, LinkFailure, Observation};
use serde::Deserialize;

/// Something Kotlin observed or finished, sent to `NativeCore.event`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Something the OS reported, for the core.
    Observed(Observation),
    /// Kotlin opened the control link the core asked for in `request`, as
    /// `link`.
    Opened { request: u64, link: i32 },
    /// Kotlin couldn't open the control link the core asked for in `request`.
    OpenFailed { request: u64, failure: LinkFailure },
    /// Bytes read from a control link, not aligned to frames.
    Data { link: i32, bytes: Vec<u8> },
    /// A control link closed: the Device or the OS dropped it.
    Closed { link: i32 },
}

/// A message Kotlin sent that isn't one the core knows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadMessage(pub String);

/// [`Event`] as Kotlin writes it.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum WireEvent {
    Connected {
        address: String,
        name: String,
        uuids: Vec<String>,
    },
    Disconnected {
        address: String,
    },
    AudioOutput {
        address: Option<String>,
    },
    Opened {
        request: u64,
        link: i32,
    },
    OpenFailed {
        request: u64,
        permission: bool,
        message: String,
    },
    Data {
        link: i32,
        bytes: Vec<u8>,
    },
    Closed {
        link: i32,
    },
}

/// Reads an event from Kotlin.
pub fn parse_event(json: &str) -> Result<Event, BadMessage> {
    let event = serde_json::from_str(json).map_err(|error| BadMessage(error.to_string()))?;
    Ok(match event {
        WireEvent::Connected {
            address,
            name,
            uuids,
        } => Event::Observed(Observation::DeviceConnected(DeviceIdentity {
            address: DeviceAddress(address),
            name,
            // A service the core can't parse is one it doesn't know.
            service_uuids: uuids.iter().filter_map(|uuid| uuid.parse().ok()).collect(),
        })),
        WireEvent::Disconnected { address } => {
            Event::Observed(Observation::DeviceDisconnected(DeviceAddress(address)))
        }
        WireEvent::AudioOutput { address } => {
            Event::Observed(Observation::AudioOutputChanged(address.map(DeviceAddress)))
        }
        WireEvent::Opened { request, link } => Event::Opened { request, link },
        WireEvent::OpenFailed {
            request,
            permission,
            message,
        } => Event::OpenFailed {
            request,
            failure: if permission {
                LinkFailure::BluetoothPermissionDenied
            } else {
                LinkFailure::Other(message)
            },
        },
        WireEvent::Data { link, bytes } => Event::Data { link, bytes },
        WireEvent::Closed { link } => Event::Closed { link },
    })
}

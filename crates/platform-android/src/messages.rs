//! The JSON messages between Kotlin and the core, and their mapping to the
//! core's types.

use headphone_bridge_core::platform::{DeviceAddress, DeviceIdentity, LinkFailure, Observation};
use headphone_bridge_core::{Command, CommandError, DeviceList, NoiseControlMode};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::json;
use uuid::Uuid;

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
    /// A surface asks the core to Attach Devices until the matching end.
    Hold(HoldKind),
    /// The surface that made the hold of that kind no longer needs it.
    EndHold(HoldKind),
}

/// What a hold asks the core to Attach. Each kind has one maker on Android,
/// so a hold needs no id: the App Screen holds every Connected Device, and the
/// widget and tile's action holds the Primary Device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HoldKind {
    All,
    Primary,
}

/// A message Kotlin sent that isn't one the core knows, with why serde
/// couldn't read it.
#[derive(Debug, thiserror::Error)]
#[error("bad message")]
pub struct BadMessage(#[source] pub serde_json::Error);

/// [`Event`] as Kotlin writes it.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum WireEvent {
    Connected {
        address: DeviceAddress,
        name: String,
        uuids: Vec<String>,
    },
    Disconnected {
        address: DeviceAddress,
    },
    AudioOutput {
        address: Option<DeviceAddress>,
    },
    Opened {
        request: u64,
        link: i32,
    },
    /// `{request, reason, detail?}`: `reason` is the link failure's serde
    /// name, and only `other` carries a `detail`.
    OpenFailed {
        request: u64,
        #[serde(flatten)]
        failure: LinkFailure,
    },
    Data {
        link: i32,
        bytes: Vec<u8>,
    },
    Closed {
        link: i32,
    },
    HoldAll,
    EndHoldAll,
    HoldPrimary,
    EndHoldPrimary,
}

/// Reads an event from Kotlin.
pub fn parse_event(json: &str) -> Result<Event, BadMessage> {
    let event = serde_json::from_str(json).map_err(BadMessage)?;
    Ok(match event {
        WireEvent::Connected {
            address,
            name,
            uuids,
        } => Event::Observed(Observation::DeviceConnected(DeviceIdentity {
            address,
            name,
            // A service the core can't parse is one it doesn't know.
            service_uuids: uuids.iter().filter_map(|uuid| uuid.parse().ok()).collect(),
        })),
        WireEvent::Disconnected { address } => {
            Event::Observed(Observation::DeviceDisconnected(address))
        }
        WireEvent::AudioOutput { address } => {
            Event::Observed(Observation::AudioOutputChanged(address))
        }
        WireEvent::Opened { request, link } => Event::Opened { request, link },
        WireEvent::OpenFailed { request, failure } => Event::OpenFailed { request, failure },
        WireEvent::Data { link, bytes } => Event::Data { link, bytes },
        WireEvent::Closed { link } => Event::Closed { link },
        WireEvent::HoldAll => Event::Hold(HoldKind::All),
        WireEvent::EndHoldAll => Event::EndHold(HoldKind::All),
        WireEvent::HoldPrimary => Event::Hold(HoldKind::Primary),
        WireEvent::EndHoldPrimary => Event::EndHold(HoldKind::Primary),
    })
}

/// [`Command`] as Kotlin writes it, sent to `NativeCore.command`. The frame
/// log is the App Screen's, so it isn't here.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum WireCommand {
    SetNoiseControlMode {
        address: DeviceAddress,
        mode: NoiseControlMode,
    },
    SetAmbientLevel {
        address: DeviceAddress,
        level: u8,
    },
    SetNoiseCancellingLevel {
        address: DeviceAddress,
        level: u8,
    },
    PowerOff {
        address: DeviceAddress,
    },
    RetryAttach {
        address: DeviceAddress,
    },
}

/// Reads a command from Kotlin.
pub fn parse_command(json: &str) -> Result<Command, BadMessage> {
    let command = serde_json::from_str(json).map_err(BadMessage)?;
    Ok(match command {
        WireCommand::SetNoiseControlMode { address, mode } => {
            Command::SetNoiseControlMode(address, mode)
        }
        WireCommand::SetAmbientLevel { address, level } => Command::SetAmbientLevel(address, level),
        WireCommand::SetNoiseCancellingLevel { address, level } => {
            Command::SetNoiseCancellingLevel(address, level)
        }
        WireCommand::PowerOff { address } => Command::PowerOff(address),
        WireCommand::RetryAttach { address } => Command::RetryAttach(address),
    })
}

/// A message from the core to Kotlin, for `NativeCore.fromCore`.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Message {
    /// Open a control link to the vendor service `service` of the Device at
    /// `address`, and answer `request` with [`Event::Opened`] or
    /// [`Event::OpenFailed`].
    Open {
        request: u64,
        address: DeviceAddress,
        #[serde(rename = "uuid")]
        service: Uuid,
    },
    /// Write `bytes` to a control link, after anything written before.
    Write { link: i32, bytes: Vec<u8> },
    /// Close a control link: the core is done with it.
    Close { link: i32 },
    /// The latest Device list snapshot.
    DeviceList { list: DeviceList },
    /// How the command Kotlin numbered `request` went.
    CommandResult {
        request: i64,
        #[serde(rename = "error", serialize_with = "command_error")]
        result: Result<(), CommandFailure>,
    },
}

impl Message {
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }
}

/// Why a command from Kotlin did nothing.
#[derive(Debug)]
pub enum CommandFailure {
    /// The core rejected it.
    Rejected(CommandError),
    /// It wasn't a command the core knows.
    BadRequest(BadMessage),
}

/// `null` for a command that was accepted, else the core's [`CommandError`]
/// as the window sees it, or a `badRequest`.
fn command_error<S: Serializer>(
    result: &Result<(), CommandFailure>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match result {
        Ok(()) => serializer.serialize_none(),
        Err(CommandFailure::Rejected(error)) => error.serialize(serializer),
        Err(CommandFailure::BadRequest(BadMessage(error))) => {
            json!({ "kind": "badRequest", "message": error.to_string() }).serialize(serializer)
        }
    }
}

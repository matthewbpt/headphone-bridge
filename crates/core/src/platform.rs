//! The platform port: everything OS-specific the core depends on.
//!
//! The port has two halves: the [`Observations`] the OS reports, which the
//! core is handed once as it starts, and a [`Platform`], which opens control
//! links whenever the core asks. An adapter per OS makes both; tests use a
//! scripted fake. The port knows nothing about vendor protocols: it reports
//! what the OS sees and opens byte streams.

use std::fmt;
use std::future::Future;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use uuid::Uuid;

pub trait Platform: Send + Sync + 'static {
    /// Opens a control link to the vendor service `service` of the Connected
    /// Device at `address`.
    fn open_control_link(
        &self,
        address: DeviceAddress,
        service: Uuid,
    ) -> impl Future<Output = Result<ControlLink, LinkFailure>> + Send;
}

/// The stream of what the OS reports. Once it ends, the core observes nothing
/// more and carries on.
pub type Observations = mpsc::UnboundedReceiver<Observation>;

/// Something the OS reports. The core only observes; it never pairs, connects
/// or disconnects Devices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observation {
    DeviceConnected(DeviceIdentity),
    DeviceDisconnected(DeviceAddress),
    /// The OS switched its audio output, to a Bluetooth Device at the given
    /// address or (`None`) to something else, such as the built-in speakers.
    AudioOutputChanged(Option<DeviceAddress>),
}

/// What the OS knows about a Connected Device before anything is Attached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceIdentity {
    pub address: DeviceAddress,
    pub name: String,
    pub service_uuids: Vec<Uuid>,
}

/// A Bluetooth device address. It holds only a well-formed address, and
/// shows as `AA:BB:CC:DD:EE:FF`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize)]
#[serde(try_from = "String")]
pub struct DeviceAddress([u8; 6]);

impl DeviceAddress {
    /// The address with these octets, most significant first.
    pub const fn from_octets(octets: [u8; 6]) -> DeviceAddress {
        DeviceAddress(octets)
    }

    /// Its octets, most significant first.
    pub const fn octets(self) -> [u8; 6] {
        self.0
    }

    /// Parses six two-digit hex octets in either case, all separated by `:`
    /// or all by `-`.
    pub fn parse(text: &str) -> Result<DeviceAddress, AddressError> {
        let malformed = || AddressError(text.to_owned());
        let separator = if text.contains('-') { '-' } else { ':' };
        let mut parts = text.split(separator);
        let mut octets = [0; 6];
        for octet in &mut octets {
            let part = parts.next().ok_or_else(malformed)?;
            if part.len() != 2 || !part.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(malformed());
            }
            *octet = u8::from_str_radix(part, 16).map_err(|_| malformed())?;
        }
        if parts.next().is_some() {
            return Err(malformed());
        }
        Ok(DeviceAddress(octets))
    }
}

impl FromStr for DeviceAddress {
    type Err = AddressError;

    fn from_str(text: &str) -> Result<DeviceAddress, AddressError> {
        DeviceAddress::parse(text)
    }
}

impl TryFrom<String> for DeviceAddress {
    type Error = AddressError;

    fn try_from(text: String) -> Result<DeviceAddress, AddressError> {
        DeviceAddress::parse(&text)
    }
}

impl fmt::Display for DeviceAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [first, rest @ ..] = self.0;
        write!(f, "{first:02X}")?;
        for octet in rest {
            write!(f, ":{octet:02X}")?;
        }
        Ok(())
    }
}

impl Serialize for DeviceAddress {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// Text that isn't a Bluetooth device address, carrying the text.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("not a Bluetooth address: {0:?}")]
pub struct AddressError(pub String);

/// An open, bidirectional control link. Chunks in `incoming` are not aligned
/// to frames. The link has dropped when `incoming` yields `None` or a send on
/// `outgoing` fails; dropping both ends closes it.
#[derive(Debug)]
pub struct ControlLink {
    pub incoming: mpsc::UnboundedReceiver<Vec<u8>>,
    pub outgoing: mpsc::UnboundedSender<Vec<u8>>,
}

/// Why a control link couldn't be opened. Its `Display` is for logs; the
/// shell words it for the listener. Its serde names are the ones the UI is
/// sent in `NotAttachedReason`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Deserialize)]
#[serde(tag = "reason", content = "detail", rename_all = "camelCase")]
pub enum LinkFailure {
    /// Another process, such as the vendor's desktop app, holds the link.
    #[error("another process holds the control link")]
    InUseByAnotherProcess,
    /// The OS hasn't let the app use Bluetooth.
    #[error("Bluetooth permission denied")]
    BluetoothPermissionDenied,
    /// The Device doesn't offer the vendor service asked for.
    #[error("the Device has no such vendor service")]
    ServiceNotFound,
    /// The Device stopped being Connected, or the OS doesn't know it.
    #[error("the Device isn't Connected")]
    DeviceNotConnected,
    /// Bluetooth Unavailable: the OS's Bluetooth is off or not ready yet.
    #[error("Bluetooth is unavailable")]
    BluetoothUnavailable,
    /// The Device didn't answer in time while the link was opening.
    #[error("timed out opening the control link")]
    TimedOut,
    /// The OS is busy with the Device, e.g. still connecting its audio. An
    /// adapter may ask again itself before reporting it.
    #[error("Bluetooth stayed busy with the Device")]
    Busy,
    /// The app's own Bluetooth handling stopped, such as the thread or task
    /// that opens links. The adapter logs the specifics where it fails.
    #[error("the app's Bluetooth handling stopped")]
    BluetoothHandlingStopped,
    /// A failure the adapter has no cause for, with its own detail.
    #[error("{0}")]
    Other(String),
}

impl LinkFailure {
    /// Whether trying again by itself might open the link. Not when only the
    /// listener can fix the cause, e.g. by quitting the app holding the link,
    /// or restarting the app whose Bluetooth handling stopped.
    pub(crate) fn is_transient(&self) -> bool {
        match self {
            LinkFailure::InUseByAnotherProcess
            | LinkFailure::BluetoothPermissionDenied
            | LinkFailure::BluetoothHandlingStopped => false,
            LinkFailure::ServiceNotFound
            | LinkFailure::DeviceNotConnected
            | LinkFailure::BluetoothUnavailable
            | LinkFailure::TimedOut
            | LinkFailure::Busy
            | LinkFailure::Other(_) => true,
        }
    }
}

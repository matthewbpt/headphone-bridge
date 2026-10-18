//! The platform port: everything OS-specific the core depends on.
//!
//! An adapter per OS implements [`Platform`]; tests use a scripted fake. The
//! port knows nothing about vendor protocols: it reports what the OS sees and
//! opens byte streams.

use std::fmt;
use std::future::Future;

use serde::Serialize;
use tokio::sync::mpsc;
use uuid::Uuid;

pub trait Platform: Send + Sync + 'static {
    /// The stream of OS observations. The core calls this once, at start; a
    /// later call may get a stream that never yields.
    fn observations(&self) -> Observations;

    /// Opens a control link to the vendor service `service` of the Connected
    /// Device at `address`.
    fn open_control_link(
        &self,
        address: &DeviceAddress,
        service: Uuid,
    ) -> impl Future<Output = Result<ControlLink, LinkFailure>> + Send;
}

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

/// A Bluetooth device address, in the form `AA:BB:CC:DD:EE:FF`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct DeviceAddress(pub String);

impl fmt::Display for DeviceAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// An open, bidirectional control link. Chunks in `incoming` are not aligned
/// to frames. The link has dropped when `incoming` yields `None` or a send on
/// `outgoing` fails; dropping both ends closes it.
#[derive(Debug)]
pub struct ControlLink {
    pub incoming: mpsc::UnboundedReceiver<Vec<u8>>,
    pub outgoing: mpsc::UnboundedSender<Vec<u8>>,
}

/// Why a control link couldn't be opened. Its `Display` is for logs; the
/// shell words it for the listener.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LinkFailure {
    /// Another process, such as the vendor's desktop app, holds the link.
    #[error("another process holds the control link")]
    InUseByAnotherProcess,
    #[error("Bluetooth permission denied")]
    BluetoothPermissionDenied,
    #[error("the Device has no such vendor service")]
    ServiceNotFound,
    #[error("the Device isn't Connected")]
    DeviceNotConnected,
    /// A failure the adapter has no cause for, with its own detail.
    #[error("{0}")]
    Other(String),
}

impl LinkFailure {
    /// Whether trying again by itself might open the link. Not when only the
    /// listener can fix the cause, e.g. by quitting the app holding the link.
    pub fn is_transient(&self) -> bool {
        match self {
            LinkFailure::InUseByAnotherProcess | LinkFailure::BluetoothPermissionDenied => false,
            LinkFailure::ServiceNotFound
            | LinkFailure::DeviceNotConnected
            | LinkFailure::Other(_) => true,
        }
    }
}

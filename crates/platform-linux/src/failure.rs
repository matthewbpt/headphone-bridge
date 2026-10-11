//! BlueZ's errors, as the port's failure reasons. Pure, so it builds and is
//! tested on every OS.

use headphone_bridge_core::platform::LinkFailure;

/// What a failed `Device1.ConnectProfile` means for opening a control link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectProfileFailure {
    /// BlueZ is busy with the Device, typically still connecting its audio
    /// profiles just after it connected. Worth asking again shortly.
    Busy,
    Fail(LinkFailure),
}

/// Maps a D-Bus error from `ConnectProfile`, by its full name
/// (`org.bluez.Error.Failed`) and BlueZ's message (`br-connection-busy`).
///
/// The mapping follows BlueZ's `device.c`/`error.c`; "in use" is BlueZ failing
/// to open an RFCOMM channel another process already holds.
pub fn connect_profile_failure(name: &str, message: &str) -> ConnectProfileFailure {
    use ConnectProfileFailure::{Busy, Fail};
    let error = name.strip_prefix("org.bluez.Error.");
    match (error, message) {
        (Some("InProgress"), _) => Busy,
        (Some("Failed"), "br-connection-busy" | "br-connection-refused")
        | (Some("AlreadyConnected"), _) => Fail(LinkFailure::InUseByAnotherProcess),
        (Some("BREDR.ProfileUnavailable"), _) | (Some("Failed"), "br-connection-sdp-search") => {
            Fail(LinkFailure::ServiceNotFound)
        }
        (
            Some("Failed"),
            "br-connection-page-timeout"
            | "br-connection-aborted-by-remote"
            | "br-connection-timeout",
        )
        | (Some("NotReady" | "DoesNotExist"), _) => Fail(LinkFailure::DeviceNotConnected),
        (Some("NotPermitted" | "NotAuthorized"), _) => Fail(LinkFailure::BluetoothPermissionDenied),
        _ => match name {
            "org.freedesktop.DBus.Error.UnknownObject" => Fail(LinkFailure::DeviceNotConnected),
            "org.freedesktop.DBus.Error.AccessDenied" => {
                Fail(LinkFailure::BluetoothPermissionDenied)
            }
            _ if message.is_empty() => Fail(LinkFailure::Other(name.to_string())),
            _ => Fail(LinkFailure::Other(format!("{name}: {message}"))),
        },
    }
}

//! BlueZ's errors, as the port's failure reasons. Pure, so it builds and is
//! tested on every OS.

use headphone_bridge_core::platform::LinkFailure;

/// Maps a D-Bus error from `ConnectProfile`, by its full name
/// (`org.bluez.Error.Failed`) and BlueZ's message (`br-connection-busy`).
///
/// The mapping follows BlueZ's `device.c`/`error.c`; "in use" is BlueZ failing
/// to open an RFCOMM channel another process already holds. [`LinkFailure::Busy`]
/// is BlueZ busy with the Device, typically still connecting its audio
/// profiles just after it connected, and is worth asking again shortly.
pub fn connect_profile_failure(name: &str, message: &str) -> LinkFailure {
    let error = name.strip_prefix("org.bluez.Error.");
    match (error, message) {
        (Some("InProgress"), _) => LinkFailure::Busy,
        (Some("Failed"), "br-connection-busy" | "br-connection-refused")
        | (Some("AlreadyConnected"), _) => LinkFailure::InUseByAnotherProcess,
        (Some("BREDR.ProfileUnavailable"), _) | (Some("Failed"), "br-connection-sdp-search") => {
            LinkFailure::ServiceNotFound
        }
        (Some("NotReady"), _) => LinkFailure::BluetoothUnavailable,
        (
            Some("Failed"),
            "br-connection-page-timeout"
            | "br-connection-aborted-by-remote"
            | "br-connection-timeout",
        )
        | (Some("DoesNotExist"), _) => LinkFailure::DeviceNotConnected,
        (Some("NotPermitted" | "NotAuthorized"), _) => LinkFailure::BluetoothPermissionDenied,
        _ => match name {
            "org.freedesktop.DBus.Error.UnknownObject" => LinkFailure::DeviceNotConnected,
            "org.freedesktop.DBus.Error.AccessDenied" => LinkFailure::BluetoothPermissionDenied,
            _ if message.is_empty() => LinkFailure::Other(name.to_string()),
            _ => LinkFailure::Other(format!("{name}: {message}")),
        },
    }
}

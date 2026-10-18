//! How BlueZ's replies to ConnectProfile become the port's failure reasons.

#![cfg(test)]

use headphone_bridge_core::platform::LinkFailure;
use headphone_bridge_platform_linux::{connect_profile_failure, ConnectProfileFailure};

fn failure(name: &str, message: &str) -> ConnectProfileFailure {
    connect_profile_failure(name, message)
}

fn fails(link_failure: LinkFailure) -> ConnectProfileFailure {
    ConnectProfileFailure::Fail(link_failure)
}

#[test]
fn bluez_busy_with_the_device_is_retried() {
    assert_eq!(
        failure("org.bluez.Error.InProgress", "br-connection-busy"),
        ConnectProfileFailure::Busy
    );
}

#[test]
fn a_link_held_by_another_process_is_in_use() {
    assert_eq!(
        failure("org.bluez.Error.Failed", "br-connection-busy"),
        fails(LinkFailure::InUseByAnotherProcess)
    );
    assert_eq!(
        failure("org.bluez.Error.Failed", "br-connection-refused"),
        fails(LinkFailure::InUseByAnotherProcess)
    );
    assert_eq!(
        failure("org.bluez.Error.AlreadyConnected", ""),
        fails(LinkFailure::InUseByAnotherProcess)
    );
}

#[test]
fn a_device_without_the_vendor_service_has_no_service() {
    assert_eq!(
        failure(
            "org.bluez.Error.BREDR.ProfileUnavailable",
            "No more profiles to connect to"
        ),
        fails(LinkFailure::ServiceNotFound)
    );
    assert_eq!(
        failure("org.bluez.Error.Failed", "br-connection-sdp-search"),
        fails(LinkFailure::ServiceNotFound)
    );
}

#[test]
fn a_device_that_went_away_is_not_connected() {
    for message in [
        "br-connection-page-timeout",
        "br-connection-aborted-by-remote",
        "br-connection-timeout",
    ] {
        assert_eq!(
            failure("org.bluez.Error.Failed", message),
            fails(LinkFailure::DeviceNotConnected),
            "{message}"
        );
    }
    assert_eq!(
        failure(
            "org.bluez.Error.NotReady",
            "br-connection-adapter-not-powered"
        ),
        fails(LinkFailure::DeviceNotConnected)
    );
    assert_eq!(
        failure("org.freedesktop.DBus.Error.UnknownObject", ""),
        fails(LinkFailure::DeviceNotConnected)
    );
    assert_eq!(
        failure("org.bluez.Error.DoesNotExist", ""),
        fails(LinkFailure::DeviceNotConnected)
    );
}

#[test]
fn a_policy_that_forbids_talking_to_bluez_denies_permission() {
    assert_eq!(
        failure(
            "org.freedesktop.DBus.Error.AccessDenied",
            "Rejected send message"
        ),
        fails(LinkFailure::BluetoothPermissionDenied)
    );
    assert_eq!(
        failure("org.bluez.Error.NotPermitted", ""),
        fails(LinkFailure::BluetoothPermissionDenied)
    );
    assert_eq!(
        failure("org.bluez.Error.NotAuthorized", ""),
        fails(LinkFailure::BluetoothPermissionDenied)
    );
}

#[test]
fn anything_else_keeps_bluezs_words() {
    assert_eq!(
        failure("org.bluez.Error.Failed", "br-connection-key-missing"),
        fails(LinkFailure::Other(
            "org.bluez.Error.Failed: br-connection-key-missing".into()
        ))
    );
    assert_eq!(
        failure("org.freedesktop.DBus.Error.ServiceUnknown", ""),
        fails(LinkFailure::Other(
            "org.freedesktop.DBus.Error.ServiceUnknown".into()
        ))
    );
}

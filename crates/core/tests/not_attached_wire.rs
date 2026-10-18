#![cfg(test)]

use headphone_bridge_core::platform::LinkFailure;
use headphone_bridge_core::NotAttachedReason;
use serde_json::{json, to_value};

#[test]
fn every_reason_reaches_the_ui_as_a_flat_kind() {
    for (reason, kind) in [
        (NotAttachedReason::UnsupportedModel, "unsupportedModel"),
        (NotAttachedReason::LinkDropped, "linkDropped"),
        (NotAttachedReason::NoResponse, "noResponse"),
        (
            NotAttachedReason::LinkFailed(LinkFailure::InUseByAnotherProcess),
            "inUseByAnotherProcess",
        ),
        (
            NotAttachedReason::LinkFailed(LinkFailure::BluetoothPermissionDenied),
            "bluetoothPermissionDenied",
        ),
        (
            NotAttachedReason::LinkFailed(LinkFailure::ServiceNotFound),
            "serviceNotFound",
        ),
        (
            NotAttachedReason::LinkFailed(LinkFailure::DeviceNotConnected),
            "deviceNotConnected",
        ),
        (
            NotAttachedReason::LinkFailed(LinkFailure::BluetoothUnavailable),
            "bluetoothUnavailable",
        ),
        (
            NotAttachedReason::LinkFailed(LinkFailure::TimedOut),
            "timedOut",
        ),
        (NotAttachedReason::LinkFailed(LinkFailure::Busy), "busy"),
        (
            NotAttachedReason::LinkFailed(LinkFailure::BluetoothHandlingStopped),
            "bluetoothHandlingStopped",
        ),
    ] {
        assert_eq!(to_value(&reason).unwrap(), json!({ "kind": kind }));
    }
}

#[test]
fn another_link_failure_reaches_the_ui_with_its_detail() {
    assert_eq!(
        to_value(NotAttachedReason::LinkFailed(LinkFailure::Other(
            "timed out".into()
        )))
        .unwrap(),
        json!({ "kind": "other", "detail": "timed out" })
    );
}

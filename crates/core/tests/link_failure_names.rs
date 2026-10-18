#![cfg(test)]

use headphone_bridge_core::platform::LinkFailure;
use headphone_bridge_core::NotAttachedReason;
use serde_json::{from_value, json, to_value};

/// A link failure goes out to the UI under the same name an adapter reads it
/// by, so both wires spell each cause alike.
#[test]
fn a_link_failure_is_read_by_the_name_the_ui_is_sent() {
    for failure in [
        LinkFailure::InUseByAnotherProcess,
        LinkFailure::BluetoothPermissionDenied,
        LinkFailure::ServiceNotFound,
        LinkFailure::DeviceNotConnected,
        LinkFailure::Other("timed out".into()),
    ] {
        let sent = to_value(NotAttachedReason::LinkFailed(failure.clone())).unwrap();
        let mut read = json!({ "reason": sent["kind"] });
        if let Some(detail) = sent.get("detail") {
            read["detail"] = detail.clone();
        }
        assert_eq!(from_value::<LinkFailure>(read).unwrap(), failure);
    }
}

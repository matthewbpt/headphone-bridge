use headphone_bridge_core::platform::{DeviceAddress, DeviceIdentity, LinkFailure, Observation};
use headphone_bridge_platform_android::messages::{parse_event, Event};

fn address(text: &str) -> DeviceAddress {
    DeviceAddress(text.into())
}

#[test]
fn a_connected_device_is_observed_with_its_service_uuids() {
    let event = parse_event(
        r#"{"type":"connected","address":"AA:BB:CC:DD:EE:FF","name":"Galaxy Buds4 Pro",
            "uuids":["2e73a4ad-332d-41fc-90e2-16bef06523f2","not-a-uuid"]}"#,
    )
    .unwrap();
    assert_eq!(
        event,
        Event::Observed(Observation::DeviceConnected(DeviceIdentity {
            address: address("AA:BB:CC:DD:EE:FF"),
            name: "Galaxy Buds4 Pro".into(),
            service_uuids: vec!["2e73a4ad-332d-41fc-90e2-16bef06523f2".parse().unwrap()],
        }))
    );
}

#[test]
fn a_disconnected_device_is_observed() {
    assert_eq!(
        parse_event(r#"{"type":"disconnected","address":"AA:BB:CC:DD:EE:FF"}"#).unwrap(),
        Event::Observed(Observation::DeviceDisconnected(address("AA:BB:CC:DD:EE:FF")))
    );
}

#[test]
fn the_media_output_is_observed_as_a_device_or_none() {
    assert_eq!(
        parse_event(r#"{"type":"audioOutput","address":"AA:BB:CC:DD:EE:FF"}"#).unwrap(),
        Event::Observed(Observation::AudioOutputChanged(Some(address(
            "AA:BB:CC:DD:EE:FF"
        ))))
    );
    assert_eq!(
        parse_event(r#"{"type":"audioOutput","address":null}"#).unwrap(),
        Event::Observed(Observation::AudioOutputChanged(None))
    );
}

#[test]
fn an_opened_link_answers_its_request() {
    assert_eq!(
        parse_event(r#"{"type":"opened","request":3,"link":7}"#).unwrap(),
        Event::Opened { request: 3, link: 7 }
    );
}

#[test]
fn a_failed_open_is_for_want_of_permission_or_else_says_why() {
    assert_eq!(
        parse_event(r#"{"type":"openFailed","request":3,"permission":true,"message":"denied"}"#)
            .unwrap(),
        Event::OpenFailed {
            request: 3,
            failure: LinkFailure::BluetoothPermissionDenied
        }
    );
    assert_eq!(
        parse_event(
            r#"{"type":"openFailed","request":4,"permission":false,"message":"secure: IOException"}"#
        )
        .unwrap(),
        Event::OpenFailed {
            request: 4,
            failure: LinkFailure::Other("secure: IOException".into())
        }
    );
}

#[test]
fn a_link_carries_bytes_until_it_closes() {
    assert_eq!(
        parse_event(r#"{"type":"data","link":7,"bytes":[0,253,255]}"#).unwrap(),
        Event::Data {
            link: 7,
            bytes: vec![0x00, 0xfd, 0xff]
        }
    );
    assert_eq!(
        parse_event(r#"{"type":"closed","link":7}"#).unwrap(),
        Event::Closed { link: 7 }
    );
}

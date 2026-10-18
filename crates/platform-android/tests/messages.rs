#![cfg(test)]

use headphone_bridge_core::platform::{DeviceAddress, DeviceIdentity, LinkFailure, Observation};
use headphone_bridge_core::{Command, CommandError, DeviceList, NoiseControlMode};
use headphone_bridge_platform_android::messages::{
    parse_command, parse_event, BadMessage, CommandFailure, Event, HoldKind, Message,
};
use serde_json::{json, Value};

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
        Event::Observed(Observation::DeviceDisconnected(address(
            "AA:BB:CC:DD:EE:FF"
        )))
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
        Event::Opened {
            request: 3,
            link: 7
        }
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

#[test]
fn holds_on_every_connected_device_or_the_primary_device_have_matching_ends() {
    assert_eq!(
        parse_event(r#"{"type":"holdAll"}"#).unwrap(),
        Event::Hold(HoldKind::All)
    );
    assert_eq!(
        parse_event(r#"{"type":"endHoldAll"}"#).unwrap(),
        Event::EndHold(HoldKind::All)
    );
    assert_eq!(
        parse_event(r#"{"type":"holdPrimary"}"#).unwrap(),
        Event::Hold(HoldKind::Primary)
    );
    assert_eq!(
        parse_event(r#"{"type":"endHoldPrimary"}"#).unwrap(),
        Event::EndHold(HoldKind::Primary)
    );
}

#[test]
fn an_event_the_core_does_not_know_is_refused() {
    assert!(parse_event("not json").is_err());
    assert!(parse_event(r#"{"type":"teleport"}"#).is_err());
    assert!(parse_event(r#"{"type":"opened","request":3}"#).is_err());
    assert!(parse_event(r#"{"type":"data","link":7,"bytes":[256]}"#).is_err());
}

#[test]
fn every_device_command_can_be_sent() {
    let device = || address("AA:BB:CC:DD:EE:FF");
    let cases = [
        (
            r#"{"kind":"setNoiseControlMode","address":"AA:BB:CC:DD:EE:FF","mode":"ambient"}"#,
            Command::SetNoiseControlMode(device(), NoiseControlMode::Ambient),
        ),
        (
            r#"{"kind":"setAmbientLevel","address":"AA:BB:CC:DD:EE:FF","level":3}"#,
            Command::SetAmbientLevel(device(), 3),
        ),
        (
            r#"{"kind":"setNoiseCancellingLevel","address":"AA:BB:CC:DD:EE:FF","level":2}"#,
            Command::SetNoiseCancellingLevel(device(), 2),
        ),
        (
            r#"{"kind":"powerOff","address":"AA:BB:CC:DD:EE:FF"}"#,
            Command::PowerOff(device()),
        ),
        (
            r#"{"kind":"retryAttach","address":"AA:BB:CC:DD:EE:FF"}"#,
            Command::RetryAttach(device()),
        ),
    ];
    for (json, command) in cases {
        assert_eq!(parse_command(json).unwrap(), command, "{json}");
    }
}

#[test]
fn a_command_the_core_does_not_know_is_refused() {
    assert!(parse_command(r#"{"kind":"setFrameLog","path":"/tmp/log"}"#).is_err());
    assert!(parse_command(
        r#"{"kind":"setNoiseControlMode","address":"AA:BB:CC:DD:EE:FF","mode":"loud"}"#
    )
    .is_err());
}

fn sent(message: Message) -> Value {
    serde_json::from_str(&message.to_json().unwrap()).unwrap()
}

#[test]
fn the_core_asks_kotlin_to_open_write_and_close_control_links() {
    assert_eq!(
        sent(Message::Open {
            request: 3,
            address: address("AA:BB:CC:DD:EE:FF"),
            service: "956c7b26-d49a-4ba8-b03f-b17d393cb6e2".parse().unwrap(),
        }),
        json!({"type": "open", "request": 3, "address": "AA:BB:CC:DD:EE:FF",
               "uuid": "956c7b26-d49a-4ba8-b03f-b17d393cb6e2"})
    );
    assert_eq!(
        sent(Message::Write {
            link: 7,
            bytes: vec![0x3e, 0x0c, 0xff]
        }),
        json!({"type": "write", "link": 7, "bytes": [62, 12, 255]})
    );
    assert_eq!(
        sent(Message::Close { link: 7 }),
        json!({"type": "close", "link": 7})
    );
}

#[test]
fn the_device_list_goes_to_kotlin_as_the_window_sees_it() {
    assert_eq!(
        sent(Message::DeviceList {
            list: DeviceList::default()
        }),
        json!({"type": "deviceList", "list": {"devices": [], "primary": null}})
    );
}

#[test]
fn a_command_result_is_null_or_why_it_failed() {
    assert_eq!(
        sent(Message::CommandResult {
            request: 5,
            result: Ok(())
        }),
        json!({"type": "commandResult", "request": 5, "error": null})
    );
    assert_eq!(
        sent(Message::CommandResult {
            request: 6,
            result: Err(CommandFailure::Rejected(CommandError::NotAttached))
        }),
        json!({"type": "commandResult", "request": 6, "error": "notAttached"})
    );
    assert_eq!(
        sent(Message::CommandResult {
            request: 7,
            result: Err(CommandFailure::BadRequest(BadMessage(
                "missing field".into()
            )))
        }),
        json!({"type": "commandResult", "request": 7,
               "error": {"kind": "badRequest", "message": "missing field"}})
    );
}

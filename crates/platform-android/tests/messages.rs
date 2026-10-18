#![cfg(test)]

use headphone_bridge_core::platform::{DeviceAddress, DeviceIdentity, LinkFailure, Observation};
use headphone_bridge_core::{
    Command, CommandError, CommandKind, DeviceCommand, DeviceList, Equalizer, NoiseControlMode,
    PresetId,
};
use headphone_bridge_platform_android::messages::{
    parse_command, parse_event, BadMessage, CommandFailure, Event, HoldKind, Message,
};
use serde_json::{json, Value};

fn address(text: &str) -> DeviceAddress {
    DeviceAddress::parse(text).unwrap()
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
fn a_failed_open_says_why_by_its_link_failure_name() {
    for (reason, failure) in [
        ("inUseByAnotherProcess", LinkFailure::InUseByAnotherProcess),
        (
            "bluetoothPermissionDenied",
            LinkFailure::BluetoothPermissionDenied,
        ),
        ("serviceNotFound", LinkFailure::ServiceNotFound),
        ("deviceNotConnected", LinkFailure::DeviceNotConnected),
        ("bluetoothUnavailable", LinkFailure::BluetoothUnavailable),
    ] {
        assert_eq!(
            parse_event(&format!(
                r#"{{"type":"openFailed","request":3,"reason":"{reason}"}}"#
            ))
            .unwrap(),
            Event::OpenFailed {
                request: 3,
                failure
            }
        );
    }
}

#[test]
fn a_failed_open_for_a_reason_the_core_does_not_know_is_refused() {
    assert!(matches!(
        parse_event(r#"{"type":"openFailed","request":3,"reason":"cosmicRays"}"#),
        Err(BadMessage(_))
    ));
}

#[test]
fn a_failed_open_whose_detail_does_not_fit_its_reason_is_refused() {
    assert!(matches!(
        parse_event(r#"{"type":"openFailed","request":3,"reason":"other"}"#),
        Err(BadMessage(_))
    ));
    assert!(matches!(
        parse_event(
            r#"{"type":"openFailed","request":3,"reason":"bluetoothPermissionDenied","detail":"denied"}"#
        ),
        Err(BadMessage(_))
    ));
}

#[test]
fn a_failed_open_without_a_cause_carries_its_detail() {
    assert_eq!(
        parse_event(
            r#"{"type":"openFailed","request":4,"reason":"other","detail":"secure: IOException"}"#
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
fn an_event_with_a_malformed_address_is_a_bad_message() {
    for json in [
        r#"{"type":"connected","address":"AA:BB:CC:DD:EE","name":"Galaxy Buds4 Pro","uuids":[]}"#,
        r#"{"type":"disconnected","address":"not an address"}"#,
        r#"{"type":"audioOutput","address":"AA_BB_CC_DD_EE_FF"}"#,
    ] {
        assert!(matches!(parse_event(json), Err(BadMessage(_))), "{json}");
    }
}

#[test]
fn a_bad_message_keeps_the_json_error_as_its_source() {
    for bad in [
        parse_event("not json").unwrap_err(),
        parse_command(r#"{"kind":"powerOff"}"#).unwrap_err(),
    ] {
        let source = std::error::Error::source(&bad).expect("a source");
        assert!(source.is::<serde_json::Error>(), "{source:?}");
        // The source isn't repeated, so a log of the chain shows it once.
        assert_eq!(bad.to_string(), "bad message");
    }
}

#[test]
fn every_device_command_can_be_sent() {
    let device = |kind| {
        Command::Device(DeviceCommand {
            address: address("AA:BB:CC:DD:EE:FF"),
            kind,
        })
    };
    let cases = [
        (
            r#"{"kind":"setNoiseControlMode","address":"AA:BB:CC:DD:EE:FF","mode":"ambient"}"#,
            device(CommandKind::SetNoiseControlMode {
                mode: NoiseControlMode::Ambient,
            }),
        ),
        (
            r#"{"kind":"setAmbientLevel","address":"AA:BB:CC:DD:EE:FF","level":3}"#,
            device(CommandKind::SetAmbientLevel { level: 3 }),
        ),
        (
            r#"{"kind":"setNoiseCancellingLevel","address":"AA:BB:CC:DD:EE:FF","level":2}"#,
            device(CommandKind::SetNoiseCancellingLevel { level: 2 }),
        ),
        (
            r#"{"kind":"setEqualizer","address":"AA:BB:CC:DD:EE:FF",
                "equalizer":{"preset":10,"bands":[-2,0,3,1,-1],"clearBass":4}}"#,
            device(CommandKind::SetEqualizer {
                equalizer: Equalizer {
                    preset: PresetId(10),
                    bands: Some(vec![-2, 0, 3, 1, -1]),
                    clear_bass: Some(4),
                },
            }),
        ),
        (
            r#"{"kind":"powerOff","address":"AA:BB:CC:DD:EE:FF"}"#,
            device(CommandKind::PowerOff),
        ),
        (
            r#"{"kind":"retryAttach","address":"AA:BB:CC:DD:EE:FF"}"#,
            device(CommandKind::RetryAttach),
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

#[test]
fn a_command_with_a_malformed_address_is_refused() {
    for json in [
        r#"{"kind":"setNoiseControlMode","address":"AA:BB:CC:DD:EE","mode":"ambient"}"#,
        r#"{"kind":"powerOff","address":"AA:BB:CC:DD:EE:GG"}"#,
    ] {
        assert!(matches!(parse_command(json), Err(BadMessage(_))), "{json}");
    }
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
            result: Err(CommandFailure::BadRequest(
                parse_command(r#"{"kind":"powerOff"}"#).unwrap_err()
            ))
        }),
        json!({"type": "commandResult", "request": 7,
               "error": {"kind": "badRequest",
                         "message": "missing field `address` at line 1 column 19"}})
    );
    assert_eq!(
        sent(Message::CommandResult {
            request: 8,
            result: Err(CommandFailure::BadRequest(
                parse_command(r#"{"kind":"powerOff","address":"AA:BB:CC:DD:EE"}"#).unwrap_err()
            ))
        }),
        json!({"type": "commandResult", "request": 8,
               "error": {"kind": "badRequest",
                         "message": "not a Bluetooth address: \"AA:BB:CC:DD:EE\" at line 1 column 46"}})
    );
}

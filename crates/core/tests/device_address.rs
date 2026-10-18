#![cfg(test)]

use headphone_bridge_core::platform::DeviceAddress;
use serde_json::json;

fn canonical(text: &str) -> String {
    DeviceAddress::parse(text)
        .unwrap_or_else(|e| panic!("{text:?} should parse: {e}"))
        .to_string()
}

#[test]
fn canonical_input_parses_to_itself() {
    assert_eq!(canonical("AA:BB:CC:DD:EE:FF"), "AA:BB:CC:DD:EE:FF");
}

#[test]
fn lowercase_input_parses_to_uppercase() {
    assert_eq!(canonical("aa:bb:cc:dd:ee:ff"), "AA:BB:CC:DD:EE:FF");
}

#[test]
fn dashed_input_parses_to_colons() {
    assert_eq!(canonical("AA-BB-CC-DD-EE-FF"), "AA:BB:CC:DD:EE:FF");
    assert_eq!(canonical("ac-80-0a-11-22-33"), "AC:80:0A:11:22:33");
}

#[test]
fn mixed_case_input_parses_to_uppercase() {
    assert_eq!(canonical("aA:Bb:cC:01:eE:9f"), "AA:BB:CC:01:EE:9F");
}

fn assert_rejected(text: &str) {
    let error = DeviceAddress::parse(text).expect_err(&format!("{text:?} should be rejected"));
    assert_eq!(
        error.to_string(),
        format!("not a Bluetooth address: {text:?}")
    );
}

#[test]
fn mixed_separators_are_rejected() {
    assert_rejected("AA:BB-CC:DD:EE:FF");
    assert_rejected("AA-BB-CC-DD-EE:FF");
}

#[test]
fn wrong_lengths_are_rejected() {
    assert_rejected("");
    assert_rejected("AA:BB:CC:DD:EE");
    assert_rejected("AA:BB:CC:DD:EE:FF:00");
    assert_rejected("A:BB:CC:DD:EE:FF");
    assert_rejected("AAA:BB:CC:DD:EE:FF");
    assert_rejected("0A:BB:CC:DD:EE:F");
    assert_rejected("AABBCCDDEEFF");
}

#[test]
fn non_hex_digits_are_rejected() {
    assert_rejected("GA:BB:CC:DD:EE:FF");
    assert_rejected("+A:BB:CC:DD:EE:FF");
    assert_rejected("AA:BB:CC:DD:EE:-F");
}

#[test]
fn extra_text_is_rejected() {
    assert_rejected(" AA:BB:CC:DD:EE:FF");
    assert_rejected("AA:BB:CC:DD:EE:FF ");
    assert_rejected("AA:BB:CC:DD:EE:FF:");
    assert_rejected(":AA:BB:CC:DD:EE:FF");
    assert_rejected("AA:BB:CC:DD:EE:FF_1");
}

#[test]
fn underscore_separators_are_rejected() {
    assert_rejected("AA_BB_CC_DD_EE_FF");
}

#[test]
fn parses_through_from_str() {
    let address: DeviceAddress = "ac-80-0a-11-22-33".parse().unwrap();
    assert_eq!(address.to_string(), "AC:80:0A:11:22:33");
}

#[test]
fn builds_from_octets_and_gives_them_back() {
    const ADDRESS: DeviceAddress = DeviceAddress::from_octets([0xAC, 0x80, 0x0A, 0x11, 0x22, 0x33]);
    assert_eq!(ADDRESS.to_string(), "AC:80:0A:11:22:33");
    assert_eq!(ADDRESS.octets(), [0xAC, 0x80, 0x0A, 0x11, 0x22, 0x33]);
    assert_eq!(DeviceAddress::parse("AC:80:0A:11:22:33").unwrap(), ADDRESS);
}

#[test]
fn serialises_in_canonical_form() {
    let address = DeviceAddress::parse("ac-80-0a-11-22-33").unwrap();
    assert_eq!(
        serde_json::to_value(address).unwrap(),
        json!("AC:80:0A:11:22:33")
    );
}

#[test]
fn deserialises_through_the_parse() {
    let address: DeviceAddress = serde_json::from_value(json!("ac-80-0a-11-22-33")).unwrap();
    assert_eq!(address.to_string(), "AC:80:0A:11:22:33");
}

#[test]
fn round_trips_through_serde() {
    let address = DeviceAddress::parse("A1:B2:C3:D4:E5:F6").unwrap();
    let json = serde_json::to_string(&address).unwrap();
    assert_eq!(
        serde_json::from_str::<DeviceAddress>(&json).unwrap(),
        address
    );
}

#[test]
fn a_malformed_string_fails_to_deserialise() {
    let error = serde_json::from_value::<DeviceAddress>(json!("AA:BB:CC:DD:EE")).unwrap_err();
    assert!(
        error
            .to_string()
            .contains(r#"not a Bluetooth address: "AA:BB:CC:DD:EE""#),
        "{error}"
    );
}

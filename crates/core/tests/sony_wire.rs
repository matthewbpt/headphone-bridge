#![cfg(test)]

use headphone_bridge_core::wire::sony::{Sequence, FLAT_CURVE, POWER_OFF};

#[test]
fn a_sequence_flips_between_its_two_values() {
    assert_eq!(Sequence::FIRST.byte(), 0);
    assert_eq!(Sequence::FIRST.flipped().byte(), 1);
    assert_eq!(Sequence::FIRST.flipped().flipped(), Sequence::FIRST);
}

#[test]
fn an_ack_answers_with_the_other_sequence() {
    assert_eq!(Sequence::FIRST.ack().byte(), 1);
    assert_eq!(Sequence::FIRST.flipped().ack().byte(), 0);
}

#[test]
fn a_sequence_on_the_wire_is_its_byte_s_low_bit() {
    assert_eq!(Sequence::from_byte(0), Sequence::FIRST);
    assert_eq!(Sequence::from_byte(3).byte(), 1);
}

#[test]
fn the_flat_curve_is_the_step_count_then_every_step_at_zero() {
    // An XM5 reports a Preset with no curve of its own as 6 steps of 0,
    // each offset by 10 on the wire.
    assert_eq!(FLAT_CURVE, [6, 10, 10, 10, 10, 10, 10]);
}

#[test]
fn power_off_is_the_power_set_status_of_power_off_on() {
    // libmdr's `POWER_SET_STATUS`, `POWER_OFF`, `ON`.
    assert_eq!(POWER_OFF, [0x24, 0x03, 0x01]);
}

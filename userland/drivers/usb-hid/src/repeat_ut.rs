// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the auto-repeat model.

use super::*;

const SETTINGS: RepeatSettings = RepeatSettings {
    delay_ns: 100,
    period_ns: 50,
};

#[test]
fn no_repeat_before_the_delay() {
    let mut repeat = Repeat::new(SETTINGS);
    repeat.press(0x04, Some(b'a'), 1_000);
    assert_eq!(repeat.tick(1_099), None);
    assert_eq!(repeat.tick(1_100), Some(b'a'));
}

#[test]
fn repeats_at_the_period() {
    let mut repeat = Repeat::new(SETTINGS);
    repeat.press(0x04, Some(b'a'), 0);
    assert_eq!(repeat.tick(100), Some(b'a'));
    assert_eq!(repeat.tick(149), None);
    assert_eq!(repeat.tick(150), Some(b'a'));
    assert_eq!(repeat.tick(200), Some(b'a'));
}

#[test]
fn release_stops_the_repeat() {
    let mut repeat = Repeat::new(SETTINGS);
    repeat.press(0x04, Some(b'a'), 0);
    repeat.release(0x04);
    assert!(!repeat.active());
    assert_eq!(repeat.tick(1_000), None);
}

#[test]
fn a_new_ascii_key_replaces_the_held_one() {
    let mut repeat = Repeat::new(SETTINGS);
    repeat.press(0x04, Some(b'a'), 0);
    repeat.press(0x05, Some(b'b'), 10);
    assert_eq!(repeat.tick(110), Some(b'b'));
}

#[test]
fn a_non_ascii_press_does_not_cancel_the_held_letter() {
    let mut repeat = Repeat::new(SETTINGS);
    repeat.press(0x04, Some(b'a'), 0);
    repeat.press(0xe1, None, 10);
    assert_eq!(repeat.tick(100), Some(b'a'));
}

#[test]
fn a_zero_period_disables_repetition() {
    let settings = RepeatSettings {
        delay_ns: 0,
        period_ns: 0,
    };
    let mut repeat = Repeat::new(settings);
    repeat.press(0x04, Some(b'a'), 0);
    assert_eq!(repeat.tick(1_000), None);
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the boot report decoder.

use super::*;

fn report(modifiers: u8, keys: [u8; 6]) -> BootReport {
    BootReport { modifiers, keys }
}

fn keys(values: &[u8]) -> [u8; 6] {
    let mut keys = [0u8; 6];
    for (slot, value) in keys.iter_mut().zip(values.iter().copied()) {
        *slot = value;
    }
    keys
}

#[test]
fn from_bytes_reads_modifiers_and_keys() {
    let decoded = BootReport::from_bytes(&[0x02, 0x00, 0x04, 0x05, 0, 0, 0, 0]).expect("report");
    assert_eq!(decoded.modifiers, 0x02);
    assert_eq!(decoded.keys, [0x04, 0x05, 0, 0, 0, 0]);
    assert_eq!(
        BootReport::from_bytes(&[0; 7]),
        Err(crate::Error::ShortReport)
    );
}

#[test]
fn press_and_release_emit_ascii() {
    let mut keyboard = Keyboard::new();
    let pressed = keyboard.update(&report(0, keys(&[0x04])));
    assert_eq!(pressed.count, 1);
    assert_eq!(
        pressed.events[0],
        KeyEvent {
            usage: 0x04,
            pressed: true,
            ascii: Some(b'a')
        }
    );
    let released = keyboard.update(&report(0, keys(&[])));
    assert_eq!(released.count, 1);
    assert_eq!(
        released.events[0],
        KeyEvent {
            usage: 0x04,
            pressed: false,
            ascii: Some(b'a')
        }
    );
}

#[test]
fn shift_changes_the_ascii() {
    let mut keyboard = Keyboard::new();
    let events = keyboard.update(&report(0x02, keys(&[0x04])));
    assert_eq!(events.count, 2);
    assert_eq!(events.events[0].usage, 0xe1);
    assert_eq!(events.events[1].ascii, Some(b'A'));
}

#[test]
fn caps_lock_toggles_on_press() {
    let mut keyboard = Keyboard::new();
    let _ = keyboard.update(&report(0, keys(&[0x39])));
    assert!(keyboard.caps());
    let _ = keyboard.update(&report(0, keys(&[])));
    let events = keyboard.update(&report(0, keys(&[0x04])));
    assert_eq!(events.events[0].ascii, Some(b'A'));
}

#[test]
fn control_maps_letters_to_control_codes() {
    let mut keyboard = Keyboard::new();
    let events = keyboard.update(&report(0x01, keys(&[0x04])));
    assert_eq!(events.events[1].ascii, Some(0x01));
}

#[test]
fn a_rollover_report_is_ignored() {
    let mut keyboard = Keyboard::new();
    let _ = keyboard.update(&report(0, keys(&[0x04])));
    let ignored = keyboard.update(&report(0, [0x01; 6]));
    assert_eq!(ignored.count, 0);
    let released = keyboard.update(&report(0, keys(&[])));
    assert_eq!(released.events[0].usage, 0x04);
}

#[test]
fn a_reordered_report_does_not_churn() {
    let mut keyboard = Keyboard::new();
    let first = keyboard.update(&report(0, keys(&[0x04, 0x05])));
    assert_eq!(first.count, 2);
    let second = keyboard.update(&report(0, keys(&[0x05, 0x04])));
    assert_eq!(second.count, 0);
}

#[test]
fn modifier_release_is_an_event() {
    let mut keyboard = Keyboard::new();
    let _ = keyboard.update(&report(0x02, keys(&[0x04])));
    let events = keyboard.update(&report(0, keys(&[0x04])));
    assert_eq!(events.count, 1);
    assert_eq!(
        events.events[0],
        KeyEvent {
            usage: 0xe1,
            pressed: false,
            ascii: None
        }
    );
}

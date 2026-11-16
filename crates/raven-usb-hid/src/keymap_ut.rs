// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the HID usage map.

use super::*;

#[test]
fn letters_follow_shift_and_caps() {
    assert_eq!(key_to_ascii(0x04, false, false), Some(b'a'));
    assert_eq!(key_to_ascii(0x04, true, false), Some(b'A'));
    assert_eq!(key_to_ascii(0x04, false, true), Some(b'A'));
    assert_eq!(key_to_ascii(0x04, true, true), Some(b'a'));
}

#[test]
fn digits_and_shifted_punctuation() {
    assert_eq!(key_to_ascii(0x1e, false, false), Some(b'1'));
    assert_eq!(key_to_ascii(0x1e, true, false), Some(b'!'));
    assert_eq!(key_to_ascii(0x27, false, false), Some(b'0'));
    assert_eq!(key_to_ascii(0x27, true, false), Some(b')'));
    assert_eq!(key_to_ascii(0x2f, true, false), Some(b'{'));
    assert_eq!(key_to_ascii(0x31, true, false), Some(b'|'));
}

#[test]
fn control_keys_and_space() {
    assert_eq!(key_to_ascii(0x28, false, false), Some(b'\n'));
    assert_eq!(key_to_ascii(0x29, false, false), Some(0x1b));
    assert_eq!(key_to_ascii(0x2a, false, false), Some(0x08));
    assert_eq!(key_to_ascii(0x2b, false, false), Some(b'\t'));
    assert_eq!(key_to_ascii(0x2c, false, false), Some(b' '));
}

#[test]
fn modifiers_and_unknown_usages_have_no_ascii() {
    assert_eq!(key_to_ascii(0xe0, false, false), None);
    assert_eq!(key_to_ascii(0xe7, true, true), None);
    assert_eq!(key_to_ascii(0x39, false, false), None);
    assert_eq!(key_to_ascii(0x4f, false, false), None);
    assert_eq!(key_to_ascii(0x00, false, false), None);
    assert!(is_modifier(0xe0));
    assert!(is_modifier(0xe7));
    assert!(!is_modifier(0x04));
}

#[test]
fn uk_differs_on_the_iso_keys() {
    assert_eq!(
        key_to_ascii_layout(0x1f, Layout::Uk, LEFT_SHIFT, false, false),
        Some(b'"')
    );
    assert_eq!(
        key_to_ascii_layout(0x20, Layout::Uk, LEFT_SHIFT, false, false),
        Some(0xa3)
    );
    assert_eq!(
        key_to_ascii_layout(0x28, Layout::Uk, LEFT_SHIFT, false, false),
        Some(b'@')
    );
    assert_eq!(
        key_to_ascii_layout(0x31, Layout::Uk, 0, false, false),
        Some(b'#')
    );
    assert_eq!(
        key_to_ascii_layout(0x31, Layout::Uk, LEFT_SHIFT, false, false),
        Some(b'~')
    );
    assert_eq!(
        key_to_ascii_layout(0x64, Layout::Uk, LEFT_SHIFT, false, false),
        Some(b'|')
    );
    assert_eq!(
        key_to_ascii_layout(0x31, Layout::Us, 0, false, false),
        Some(b'\\')
    );
    assert_eq!(
        key_to_ascii_layout(0x1f, Layout::Us, LEFT_SHIFT, false, false),
        Some(b'@')
    );
}

#[test]
fn alt_and_altgr_suppress_ascii() {
    assert_eq!(
        key_to_ascii_layout(0x04, Layout::Us, LEFT_ALT, false, false),
        None
    );
    assert_eq!(
        key_to_ascii_layout(0x04, Layout::Us, RIGHT_ALT, false, false),
        None
    );
    assert_eq!(
        key_to_ascii_layout(0x20, Layout::Uk, LEFT_ALT | LEFT_SHIFT, false, false),
        None
    );
}

#[test]
fn num_lock_selects_keypad_digits() {
    assert_eq!(
        key_to_ascii_layout(0x59, Layout::Us, 0, false, true),
        Some(b'1')
    );
    assert_eq!(key_to_ascii_layout(0x59, Layout::Us, 0, false, false), None);
    assert_eq!(
        key_to_ascii_layout(0x62, Layout::Us, 0, false, true),
        Some(b'0')
    );
    assert_eq!(
        key_to_ascii_layout(0x63, Layout::Us, 0, false, true),
        Some(b'.')
    );
    assert_eq!(key_to_ascii_layout(0x63, Layout::Us, 0, false, false), None);
    assert_eq!(
        key_to_ascii_layout(0x57, Layout::Us, 0, false, false),
        Some(b'+')
    );
    assert_eq!(
        key_to_ascii_layout(0x58, Layout::Us, 0, false, false),
        Some(b'\n')
    );
}

#[test]
fn control_covers_punctuation() {
    assert_eq!(
        key_to_ascii_layout(0x2f, Layout::Us, LEFT_CTRL, false, false),
        Some(0x1b)
    );
    assert_eq!(
        key_to_ascii_layout(0x31, Layout::Us, LEFT_CTRL, false, false),
        Some(0x1c)
    );
    assert_eq!(
        key_to_ascii_layout(0x1f, Layout::Us, LEFT_CTRL, false, false),
        Some(0x00)
    );
    assert_eq!(
        key_to_ascii_layout(0x38, Layout::Us, LEFT_CTRL | LEFT_SHIFT, false, false),
        Some(0x7f)
    );
}

#[test]
fn layout_names_are_stable() {
    assert_eq!(Layout::Us.name(), "us");
    assert_eq!(Layout::Uk.name(), "uk");
    assert_eq!(Layout::default(), Layout::Us);
}

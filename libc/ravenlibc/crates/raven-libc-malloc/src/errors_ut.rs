// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn decimal_and_hex_formatting() {
    let mut message = Message::new();
    message.push_dec(0);
    message.push(b' ');
    message.push_dec(12345);
    message.push(b' ');
    message.push_hex(0xdead);
    message.push(b' ');
    message.push_ptr(0x10);
    assert_eq!(message.as_bytes(), b"0 12345 dead 0x10");
}

#[test]
fn bytes_and_strings_append() {
    let mut message = Message::new();
    message.push_str("raven");
    message.push_bytes(b"-bsd");
    assert_eq!(message.as_bytes(), b"raven-bsd");
}

#[test]
fn messages_are_bounded() {
    let mut message = Message::new();
    let mut index = 0usize;
    while index < MAX_MESSAGE + 20 {
        message.push(b'x');
        index += 1;
    }
    assert_eq!(message.as_bytes().len(), MAX_MESSAGE);
}

#[test]
fn pointer_format_has_the_donor_prefix() {
    let mut message = Message::new();
    message.push_ptr(0);
    assert_eq!(message.as_bytes(), b"0x0");
}

#[test]
fn decimal_handles_usize_max() {
    let mut message = Message::new();
    message.push_dec(usize::MAX);
    assert!(!message.as_bytes().is_empty());
}

#[test]
fn hex_handles_zero_and_max() {
    let mut zero = Message::new();
    zero.push_hex(0);
    assert_eq!(zero.as_bytes(), b"0");
    let mut max = Message::new();
    max.push_hex(usize::MAX);
    assert_eq!(max.as_bytes().len(), core::mem::size_of::<usize>() * 2);
}

#[test]
fn message_default_is_empty() {
    assert_eq!(Message::default().as_bytes(), b"");
}

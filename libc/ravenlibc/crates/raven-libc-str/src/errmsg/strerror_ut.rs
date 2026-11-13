// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::{message, strerror_into};

#[test]
fn message_known_and_unknown() {
    assert_eq!(message(0), Some(&b"Success"[..]));
    assert_eq!(message(2), Some(&b"No such file or directory"[..]));
    assert_eq!(message(-1), None);
    assert_eq!(message(134), None);
    assert_eq!(message(i32::MIN), None);
    assert_eq!(message(i32::MAX), None);
}

#[test]
fn strerror_into_known_is_static_text() {
    let mut buffer = [0xAAu8; super::super::BUF_LEN];
    let (text, known) = strerror_into(22, &mut buffer);
    assert!(known);
    assert_eq!(text, b"Invalid argument");
    assert_eq!(buffer, [0xAAu8; super::super::BUF_LEN]);
}

#[test]
fn strerror_into_unknown_writes_the_number() {
    for (value, expected) in [
        (-1, &b"Unknown error: -1"[..]),
        (134, b"Unknown error: 134"),
        (1000, b"Unknown error: 1000"),
        (i32::MIN, b"Unknown error: -2147483648"),
        (i32::MAX, b"Unknown error: 2147483647"),
    ] {
        let mut buffer = [0u8; super::super::BUF_LEN];
        let (text, known) = strerror_into(value, &mut buffer);
        assert!(!known);
        assert_eq!(text, expected);
        assert_eq!(text.len(), expected.len());
        assert!(text.len() < super::super::BUF_LEN);
    }
}

#[test]
fn every_representable_error_number_fits_the_buffer() {
    let mut buffer = [0u8; super::super::BUF_LEN];
    for value in [i32::MIN, i32::MIN + 1, -1000, -1, 134, 1000, i32::MAX] {
        let (text, known) = strerror_into(value, &mut buffer);
        assert!(!known);
        assert!(
            text.len() <= 26,
            "value {value} produced {} bytes",
            text.len()
        );
    }
}

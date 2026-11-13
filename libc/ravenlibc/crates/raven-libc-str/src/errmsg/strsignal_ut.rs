// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::{signal_message, strsignal_into};

#[test]
fn valid_signals_are_named() {
    assert_eq!(signal_message(1), Some(&b"Hangup"[..]));
    assert_eq!(signal_message(2), Some(&b"Interrupt"[..]));
    assert_eq!(signal_message(9), Some(&b"Killed"[..]));
    assert_eq!(signal_message(11), Some(&b"Segmentation fault"[..]));
    assert_eq!(signal_message(16), Some(&b"Stack fault"[..]));
    assert_eq!(signal_message(31), Some(&b"Bad system call"[..]));
    assert_eq!(signal_message(32), Some(&b"RT32"[..]));
    assert_eq!(signal_message(64), Some(&b"RT64"[..]));
}

#[test]
fn invalid_signals_are_unknown() {
    for value in [0i32, -1, 65, 1000, i32::MIN, i32::MAX] {
        assert_eq!(signal_message(value), None, "signal {value}");
    }
}

#[test]
fn every_valid_signal_has_text() {
    for signal in 1i32..=64 {
        let text = signal_message(signal).unwrap_or_else(|| panic!("signal {signal}"));
        assert!(!text.is_empty(), "signal {signal}");
        assert!(!text.contains(&0), "signal {signal}");
    }
}

#[test]
fn strsignal_into_known() {
    let mut buffer = [0xAAu8; super::super::BUF_LEN];
    let (text, known) = strsignal_into(9, &mut buffer);
    assert!(known);
    assert_eq!(text, b"Killed");
    assert_eq!(buffer, [0xAAu8; super::super::BUF_LEN]);
}

#[test]
fn strsignal_into_unknown() {
    let cases: [(i32, &[u8]); 5] = [
        (0, b"Unknown signal: 0"),
        (-1, b"Unknown signal: -1"),
        (65, b"Unknown signal: 65"),
        (1000, b"Unknown signal: 1000"),
        (i32::MIN, b"Unknown signal: -2147483648"),
    ];
    for (signal, expected) in cases {
        let mut buffer = [0u8; super::super::BUF_LEN];
        let (text, known) = strsignal_into(signal, &mut buffer);
        assert!(!known);
        assert_eq!(text, expected, "signal {signal}");
    }
}

#[test]
fn every_signal_in_the_ticket_domain() {
    for signal in -1i32..=70 {
        let mut buffer = [0u8; super::super::BUF_LEN];
        let (text, known) = strsignal_into(signal, &mut buffer);
        if (1..=64).contains(&signal) {
            assert!(known, "signal {signal}");
            assert!(!text.is_empty());
        } else {
            assert!(!known, "signal {signal}");
            assert!(text.starts_with(b"Unknown signal: "));
        }
    }
    let mut buffer = [0u8; super::super::BUF_LEN];
    let (text, known) = strsignal_into(1000, &mut buffer);
    assert!(!known);
    assert_eq!(text, b"Unknown signal: 1000");
}

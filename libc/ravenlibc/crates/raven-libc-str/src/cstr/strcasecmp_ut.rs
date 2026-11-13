// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::{strcasecmp, strncasecmp};

fn sign(value: i32) -> i32 {
    value.signum()
}

#[test]
fn ascii_case_folds() {
    assert_eq!(strcasecmp(b"abc", b"ABC"), 0);
    assert_eq!(strcasecmp(b"AbC", b"aBc"), 0);
    assert_eq!(strcasecmp(b"a1", b"A1"), 0);
}

#[test]
fn non_letters_are_not_folded() {
    assert_ne!(strcasecmp(b"[", b"{"), 0);
    assert_ne!(strcasecmp(b"@", b"`"), 0);
    assert_ne!(strcasecmp(b"abc", b"ab1"), 0);
}

#[test]
fn high_bytes_unchanged() {
    assert_eq!(strcasecmp(&[0x80], &[0x80]), 0);
    assert_eq!(strcasecmp(&[0xc0], &[0xe0]), -32);
    assert_eq!(strcasecmp(&[0xe0], &[0xc0]), 32);
    assert_eq!(strcasecmp(&[0x80], &[0x81]), -1);
}

#[test]
fn result_is_lowercased_difference() {
    assert_eq!(strcasecmp(b"B", b"a"), 1);
    assert_eq!(strcasecmp(b"b", b"A"), 1);
    assert_eq!(strcasecmp(b"a", b"B"), -1);
}

#[test]
fn n_limits() {
    assert_eq!(strncasecmp(b"abc", b"abd", 2), 0);
    assert_eq!(sign(strncasecmp(b"abc", b"abd", 3)), -1);
    assert_eq!(strncasecmp(b"abc", b"xyz", 0), 0);
}

#[test]
fn every_byte_pair_matches_the_table() {
    let lower = |byte: u8| {
        if byte.is_ascii_uppercase() {
            byte + 32
        } else {
            byte
        }
    };
    for left in 0u8..=255 {
        for right in 0u8..=255 {
            let expected = i32::from(lower(left)) - i32::from(lower(right));
            assert_eq!(strcasecmp(&[left], &[right]), expected, "{left} {right}");
        }
    }
}

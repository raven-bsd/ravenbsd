// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::LOWER;

#[test]
fn identity_below_upper_a() {
    for byte in 0u8..=b'@' {
        assert_eq!(LOWER[usize::from(byte)], byte);
    }
}

#[test]
fn upper_to_lower() {
    for byte in b'A'..=b'Z' {
        assert_eq!(LOWER[usize::from(byte)], byte + 32);
    }
}

#[test]
fn identity_after_z() {
    for byte in b'['..=0xffu8 {
        assert_eq!(LOWER[usize::from(byte)], byte);
    }
}

#[test]
fn high_bytes_unchanged() {
    for byte in 0x80u8..=0xff {
        assert_eq!(LOWER[usize::from(byte)], byte);
    }
}

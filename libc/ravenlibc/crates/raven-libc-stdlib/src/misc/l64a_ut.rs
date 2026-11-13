// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::assertions_on_constants
)]

//! `l64a` vectors and the inverse property.

use super::*;
use crate::misc::a64l;

#[test]
fn zero_is_the_empty_string() {
    let mut out = [0u8; 8];
    l64a(0, &mut out);
    assert_eq!(out[0], 0);
}

#[test]
fn inverse_of_a64l() {
    let mut out = [0u8; 8];
    for value in 0..100_000i64 {
        l64a(value, &mut out);
        let length = out.iter().position(|byte| *byte == 0).unwrap_or(8);
        assert_eq!(a64l(&out[..length]), Ok(value));
    }
}

#[test]
fn six_digits_maximum() {
    let mut out = [0u8; 8];
    l64a((1i64 << 36) - 1, &mut out);
    let length = out.iter().position(|byte| *byte == 0).unwrap_or(8);
    assert_eq!(length, 6);
    l64a(i64::MAX, &mut out);
    let length = out.iter().position(|byte| *byte == 0).unwrap_or(8);
    assert_eq!(length, 6);
}

#[test]
fn every_digit_boundary() {
    let mut out = [0u8; 8];
    for (value, expected) in [
        (1i64, b'/'),
        (2, b'0'),
        (11, b'9'),
        (12, b'A'),
        (37, b'Z'),
        (38, b'a'),
        (63, b'z'),
    ] {
        l64a(value, &mut out);
        assert_eq!(out[0], expected, "value {value}");
    }
}

#[test]
fn six_digit_value_roundtrip() {
    let mut out = [0u8; 8];
    l64a(0xffff_ffff, &mut out);
    let length = out.iter().position(|byte| *byte == 0).unwrap_or(8);
    assert_eq!(a64l(&out[..length]), Ok(0xffff_ffff));
}

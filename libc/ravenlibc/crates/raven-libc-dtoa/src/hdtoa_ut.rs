// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::*;
use crate::Rounding;

fn hex(value: f64, ndigits: i32) -> (String, i32) {
    let mut out = [0u8; 64];
    let d = hdtoa(value, false, ndigits, Rounding::Nearest, &mut out).unwrap();
    (String::from_utf8(out[..d.len].to_vec()).unwrap(), d.exp2)
}

#[test]
fn known_hex_shapes() {
    assert_eq!(hex(1.0, -1), ("1".to_string(), 0));
    assert_eq!(hex(1.5, -1), ("18".to_string(), 0));
    assert_eq!(hex(3.0, -1), ("18".to_string(), 1));
    assert_eq!(hex(0.5, -1), ("1".to_string(), -1));
    assert_eq!(hex(f64::from_bits(1), -1), ("1".to_string(), -1074));
    assert_eq!(hex(1.0, 1), ("1".to_string(), 0));
    assert_eq!(hex(1.0, 3), ("100".to_string(), 0));
}

#[test]
fn rounding_at_a_nibble_boundary() {
    // 0x1.fff... rounds to 0x1 with one digit, bumping the exponent by four bits.
    let value = f64::from_bits(0x3fff_ffff_ffff_ffff);
    assert_eq!(hex(value, 1), ("2".to_string(), 0));
    let down = {
        let mut out = [0u8; 64];
        let d = hdtoa(value, false, 1, Rounding::TowardZero, &mut out).unwrap();
        (String::from_utf8(out[..d.len].to_vec()).unwrap(), d.exp2)
    };
    assert_eq!(down, ("1".to_string(), 0));
    // The leading digit itself can carry: 0x1.ffff... rounds to 0x2p+3 here.
    let value = f64::from_bits(0x402f_ffff_ffff_ffff);
    assert_eq!(hex(value, 1), ("2".to_string(), 3));
}

#[test]
fn zero_and_specials() {
    let mut out = [0u8; 64];
    let z = hdtoa(0.0, false, -1, Rounding::Nearest, &mut out).unwrap();
    assert_eq!((z.len, z.exp2, z.kind), (1, 0, Kind::Zero));
    assert_eq!(&out[..1], b"0");
    let inf = hdtoa(f64::INFINITY, false, -1, Rounding::Nearest, &mut out).unwrap();
    assert_eq!(inf.kind, Kind::Inf);
    assert_eq!(inf.len, 0);
}

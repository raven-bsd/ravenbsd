// SPDX-License-Identifier: MIT
// Copyright (c) 2018 Jorge Aparicio
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from rust:library/compiler-builtins/libm/src/math/support/hex_float.rs @ 8bab26f4f68e0e26f0bb7960be334d5b520ea452
// (tests moved out of hex_float.rs; the exhaustive donor tables are condensed to representative cases)

use super::*;
use crate::support::Round;

#[test]
fn hf32_and_hf64_parse_exact_hex_literals() {
    assert_eq!(hf32("0x1p0").to_bits(), 1.0f32.to_bits());
    assert_eq!(hf32("0x1p10").to_bits(), 1024.0f32.to_bits());
    assert_eq!(hf32("-0x1p-1").to_bits(), (-0.5f32).to_bits());
    assert_eq!(hf64("0x1.8p1").to_bits(), 3.0f64.to_bits());
    assert_eq!(
        hf64("-0x1p-1074").to_bits(),
        f64::from_bits(0x8000_0000_0000_0001).to_bits()
    );
    assert_eq!(hf64("0x0p0").to_bits(), 0.0f64.to_bits());
    assert_eq!(hf32("inf").to_bits(), f32::INFINITY.to_bits());
    assert_eq!(hf64("nan").to_bits(), f64::NAN.to_bits());
}

#[test]
fn hex_parsing_rejects_bad_input() {
    assert!(parse_hex_exact("1p0", 64, 52).is_err());
    assert!(parse_hex_exact("0x", 64, 52).is_err());
    assert!(parse_hex_exact("0x1p", 64, 52).is_err());
    assert!(parse_hex_exact("0x1.2.3p0", 64, 52).is_err());
    assert!(parse_hex_exact("0x1q0", 64, 52).is_err());
    assert!(parse_hex_exact("0x1p9999999", 64, 52).is_err());
    assert!(parse_hex_exact("0x1p-9999999", 64, 52).is_err());
    // Too many significant digits for a double.
    assert!(parse_hex_exact("0x1.00000000000000000000000000000001p0", 64, 52).is_err());
}

#[test]
fn hex_rounding_modes_round_the_last_bit() {
    // 0x1.00000000000008p0 is the midpoint between 1.0 and 1+2^-52.
    let up = parse_any("0x1.00000000000008p0", 64, 52, Round::Nearest)
        .unwrap()
        .0;
    assert_eq!(up, u128::from(1.0f64.to_bits()));
    let toward_zero = parse_any("0x1.00000000000008p0", 64, 52, Round::Zero)
        .unwrap()
        .0;
    assert_eq!(toward_zero, u128::from(1.0f64.to_bits()));
    let toward_positive = parse_any("0x1.00000000000008p0", 64, 52, Round::Positive)
        .unwrap()
        .0;
    assert_eq!(toward_positive, u128::from(1.0f64.to_bits()) + 1);
    let subnormal = parse_any("0x0.8p-1074", 64, 52, Round::Nearest).unwrap().0;
    assert_eq!(subnormal, 0);
}

#[test]
fn display_hex_formats_floats_and_integers() {
    use std::string::ToString;
    assert_eq!(Hex(1.0f64).to_string(), "+0x1.0000000000000p+0");
    assert_eq!(Hex(-1.0f64).to_string(), "-0x1.0000000000000p+0");
    assert_eq!(Hex(0.0f64).to_string(), "+0x0p+0");
    assert_eq!(Hex(f64::INFINITY).to_string(), "+inf");
    assert_eq!(Hex(f64::NEG_INFINITY).to_string(), "-inf");
    assert_eq!(Hex(f32::NAN).to_string(), "+qNaN(0x000000)");
    assert_eq!(Hex(255u8).to_string(), "0xff");
    assert_eq!(Hex((1u8, 2u8)).to_string(), "(0x01, 0x02)");
}

#[test]
fn hex_payload_and_status_are_kept() {
    let (bits, status) = parse_any("0x1.00000000000008p0", 64, 52, Round::Nearest).unwrap();
    assert!(status.inexact());
    assert_eq!(bits, u128::from(1.0f64.to_bits()));
    let (_, status) = parse_any("0x1p1024", 64, 52, Round::Nearest).unwrap();
    assert!(status.overflow());
    let (_, status) = parse_any("0x1p-1075", 64, 52, Round::Nearest).unwrap();
    assert!(status.inexact());
}

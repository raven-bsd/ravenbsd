// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `long double` representation, classification, conversion and (x86-64) arithmetic tests.

use super::*;
use crate::consts::{FP_INFINITE, FP_NAN, FP_NORMAL, FP_SUBNORMAL, FP_ZERO};

fn ld(value: f64) -> LongDouble {
    LongDouble::from_f64(value)
}

#[test]
fn the_layout_is_the_target_abi() {
    assert_eq!(core::mem::size_of::<LongDouble>(), 16);
    assert_eq!(core::mem::align_of::<LongDouble>(), 16);
    // 1.0 has the biased exponent 0x3fff and the explicit integer bit set.
    let one = LongDouble::one();
    assert_eq!(one.exponent_field(), 0x3fff);
    assert_eq!(one.significand_bits(), 1 << 63);
    assert_eq!(one.to_bits(), 0x3fff_8000_0000_0000_0000u128);
    // The pattern is the low ten bytes of the 16-byte slot; the upper six are padding.
    assert_eq!(one.to_bits() >> 80, 0);
}

#[test]
fn the_classes_cover_every_encoding() {
    assert_eq!(ld(0.0).class(), FP_ZERO);
    assert_eq!(LongDouble::zero(true).class(), FP_ZERO);
    assert_eq!(ld(1.5).class(), FP_NORMAL);
    assert_eq!(LongDouble::infinity(false).class(), FP_INFINITE);
    assert_eq!(LongDouble::nan().class(), FP_NAN);
    // The smallest subnormal: exponent field 0, integer bit clear, significand 1.
    let subnormal = LongDouble::from_bits(1);
    assert_eq!(subnormal.class(), FP_SUBNORMAL);
    assert!(subnormal.is_finite());
    // A pseudo-denormal: exponent field 0 with the integer bit set.
    let pseudo = LongDouble::from_parts(false, 0, 1 << 63);
    assert_eq!(pseudo.class(), FP_SUBNORMAL);
    assert_eq!(pseudo.to_f64(), 0.0);
    // A NaN needs a payload bit beyond the integer bit.
    assert!(!LongDouble::from_bits(((0x7fffu128) << 64) | (1u128 << 63)).is_nan());
    assert!(LongDouble::from_bits(((0x7fffu128) << 64) | (1u128 << 63) | 1).is_nan());
}

#[test]
fn conversions_to_f64_are_exact_or_correctly_rounded() {
    for value in [
        0.0,
        -0.0,
        1.0,
        -1.0,
        0.1,
        1.5,
        3.0e300,
        f64::MIN_POSITIVE,
        f64::MAX,
    ] {
        let wide = ld(value);
        let back = wide.to_f64();
        assert_eq!(back.to_bits(), value.to_bits(), "{value}");
    }
    assert!(LongDouble::nan().to_f64().is_nan());
    assert_eq!(LongDouble::infinity(true).to_f64(), f64::NEG_INFINITY);
    assert_eq!(LongDouble::from_i64(-5).to_f64(), -5.0);
    assert_eq!(LongDouble::from_u64(u64::MAX).to_f64(), u64::MAX as f64);
    // Rounding a value with more than 53 bits of significand to f64.
    let exact = ld(1.0).add(LongDouble::from_bits(0x3fff_8000_0000_0000_0001));
    assert!(exact.to_f64() >= 1.0);
    assert_eq!(LongDouble::from_i64(-7).to_i64(), Some(-7));
    assert_eq!(LongDouble::from_u64(9).to_u64(), Some(9));
    assert_eq!(LongDouble::from_i64(-1).to_u64(), None);
}

#[test]
fn the_total_order_orders_zero_and_nan() {
    let series = [
        LongDouble::from_bits((1u128 << 79) | ((0x7fffu128) << 64) | (1u128 << 63) | (1 << 62) | 1),
        LongDouble::infinity(true),
        ld(-1.0),
        LongDouble::zero(true),
        LongDouble::zero(false),
        ld(1.0),
        LongDouble::infinity(false),
        LongDouble::nan(),
    ];
    for window in series.windows(2) {
        assert_eq!(
            window[0].total_cmp(window[1]),
            core::cmp::Ordering::Less,
            "{:#x} ({:#x}) then {:#x} ({:#x})",
            window[0].to_bits(),
            super::order_key(window[0]),
            window[1].to_bits(),
            super::order_key(window[1])
        );
        assert!(totalorderl(window[0], window[1]));
    }
    assert_eq!(ld(1.0).total_cmp(ld(1.0)), core::cmp::Ordering::Equal);
    assert_eq!(ld(1.0).partial_cmp(LongDouble::nan()), None);
}

#[cfg(target_arch = "x86_64")]
#[test]
fn arithmetic_matches_the_exact_f64_results() {
    let pairs = [
        (1.0, 2.0),
        (1.5, -2.25),
        (1e300, 1e300),
        (1.0, 1e-300),
        (-0.0, 0.0),
        (123.456, 0.001),
        (f64::MAX, 2.0),
    ];
    for (a, b) in pairs {
        let (x, y) = (ld(a), ld(b));
        assert_eq!(x.add(y).to_f64().to_bits(), (a + b).to_bits(), "{a} + {b}");
        assert_eq!(x.sub(y).to_f64().to_bits(), (a - b).to_bits(), "{a} - {b}");
        assert_eq!(x.mul(y).to_f64().to_bits(), (a * b).to_bits(), "{a} * {b}");
        if b != 0.0 {
            assert_eq!(x.div(y).to_f64().to_bits(), (a / b).to_bits(), "{a} / {b}");
        }
    }
    assert_eq!(ld(1.0).add(ld(2.0)).to_f64(), 3.0);
    assert_eq!(ld(1.0).div(ld(4.0)).to_f64(), 0.25);
}

#[cfg(target_arch = "x86_64")]
#[test]
fn sqrt_and_the_integral_functions_behave() {
    for value in [0.0, 1.0, 2.0, 4.0, 0.25, 1e100] {
        assert_eq!(ld(value).sqrt().to_f64().to_bits(), value.sqrt().to_bits());
    }
    assert!(ld(-1.0).sqrt().is_nan());
    assert_eq!(ld(2.5).floor().to_f64(), 2.0);
    assert_eq!(ld(2.5).ceil().to_f64(), 3.0);
    assert_eq!(ld(2.5).trunc().to_f64(), 2.0);
    assert_eq!(ld(2.5).round().to_f64(), 3.0);
    assert_eq!(ld(-2.5).round().to_f64(), -3.0);
    assert_eq!(ld(-2.5).floor().to_f64(), -3.0);
    assert_eq!(ld(-2.5).ceil().to_f64(), -2.0);
    assert_eq!(ld(0.4).round().to_f64(), 0.0);
    assert_eq!(ld(-0.4).round().to_f64(), -0.0);
}

#[cfg(target_arch = "x86_64")]
#[test]
fn fmod_and_nextafter_use_the_bit_patterns() {
    assert_eq!(ld(5.5).fmod(ld(2.0)).to_f64(), 1.5);
    assert_eq!(ld(-5.5).fmod(ld(2.0)).to_f64(), -1.5);
    assert_eq!(ld(5.0).fmod(ld(2.0)).to_f64(), 1.0);
    assert!(ld(1.0).fmod(ld(0.0)).is_nan());
    assert!(ld(1.0).fmod(LongDouble::infinity(false)).to_f64() == 1.0);
    let one = ld(1.0);
    let next = one.next_after(ld(2.0));
    assert_eq!(next.to_bits(), one.to_bits() + 1);
    assert_eq!(next.next_after(one).to_bits(), one.to_bits());
    assert_eq!(ld(0.0).next_after(ld(1.0)).to_bits(), 1);
    assert_eq!(LongDouble::nan().next_after(one).class(), FP_NAN);
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! binary128 bit-rule tests: classification, the total order, exact `f64` conversion and the
//! canonical encodings (zero, subnormal, infinity, NaN, hidden bit).

use super::*;
use crate::consts::{FP_INFINITE, FP_NAN, FP_NORMAL, FP_SUBNORMAL, FP_ZERO};

const ONE: u128 = 0x3fff_0000_0000_0000_0000_0000_0000_0000;

#[test]
fn the_canonical_encodings_have_the_hidden_bit() {
    assert_eq!(from_f64(1.0), ONE);
    assert_eq!(from_f64(-1.0), ONE | SIGN);
    assert_eq!(from_f64(0.0), 0);
    assert_eq!(from_f64(-0.0), SIGN);
    assert_eq!(classify(from_f64(1.5)), FP_NORMAL);
    assert_eq!(classify(0), FP_ZERO);
    assert_eq!(classify(SIGN), FP_ZERO);
    assert_eq!(
        classify(SIGN | (EXPONENT_MASK as u128) << EXPONENT_SHIFT),
        FP_INFINITE
    );
    assert_eq!(
        classify(SIGN | (EXPONENT_MASK as u128) << EXPONENT_SHIFT | QUIET),
        FP_NAN
    );
    // The smallest subnormal has an empty exponent field and a one-bit significand.
    assert_eq!(classify(1), FP_SUBNORMAL);
}

#[test]
fn conversions_are_exact_for_f64_values() {
    for value in [
        0.0,
        -0.0,
        1.0,
        -1.0,
        0.1,
        f64::MIN_POSITIVE,
        f64::from_bits(1),
        f64::MAX,
        1e300,
    ] {
        let wide = from_f64(value);
        assert_eq!(to_f64(wide).to_bits(), value.to_bits(), "{value}");
    }
    assert!(to_f64(from_f64(f64::NAN)).is_nan());
    assert_eq!(to_f64(from_f64(f64::NEG_INFINITY)), f64::NEG_INFINITY);
    assert_eq!(to_f64(ONE), 1.0);
}

#[test]
fn the_total_order_puts_zero_and_nan_in_place() {
    let series = [
        SIGN | (EXPONENT_MASK as u128) << EXPONENT_SHIFT | QUIET | 1, // -NaN
        SIGN | (EXPONENT_MASK as u128) << EXPONENT_SHIFT,             // -inf
        SIGN | ONE,                                                   // -1
        SIGN,                                                         // -0
        0,                                                            // +0
        ONE,                                                          // +1
        (EXPONENT_MASK as u128) << EXPONENT_SHIFT,                    // +inf
        (EXPONENT_MASK as u128) << EXPONENT_SHIFT | QUIET,            // +NaN
    ];
    for window in series.windows(2) {
        assert!(total_order(window[0], window[1]), "{:#x}", window[0]);
        assert!(!total_order(window[1], window[0]));
    }
    assert!(total_order(ONE, ONE));
}

#[test]
fn the_significand_is_the_low_112_bits() {
    // A pattern with only the quiet bit and a payload is a NaN; the integer bit is hidden and the
    // stored field is empty for infinity.
    let nan = (EXPONENT_MASK as u128) << EXPONENT_SHIFT | QUIET | 0x1234;
    assert_eq!(classify(nan), FP_NAN);
    assert_eq!(nan & SIGNIFICAND_MASK, QUIET | 0x1234);
    assert_eq!(ONE & SIGNIFICAND_MASK, 0);
}

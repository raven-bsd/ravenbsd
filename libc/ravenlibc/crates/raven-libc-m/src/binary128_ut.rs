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

#[test]
fn the_binary128_type_adds_and_subtracts_correctly() {
    use super::LongDouble128;
    let one = LongDouble128::from_f64(1.0);
    let two = LongDouble128::from_f64(2.0);
    assert_eq!(one.add(one).to_f64(), 2.0);
    assert_eq!(
        LongDouble128::from_f64(1.5)
            .add(LongDouble128::from_f64(2.25))
            .to_f64(),
        3.75
    );
    assert_eq!(one.sub(one).to_f64(), 0.0);
    assert_eq!(one.sub(two).to_f64(), -1.0);
    assert_eq!(two.sub(one).to_f64(), 1.0);
    assert!(
        LongDouble128::zero(true)
            .add(LongDouble128::zero(true))
            .sign_bit()
    );
    assert!(!one.add(two).sub(LongDouble128::from_f64(3.0)).sign_bit());
    assert!(
        LongDouble128::infinity(false)
            .add(LongDouble128::infinity(true))
            .is_nan()
    );
    assert_eq!(
        LongDouble128::infinity(false).add(one).to_f64(),
        f64::INFINITY
    );
    assert!(one.add(LongDouble128::nan()).is_nan());
    // Exact in 113 bits: 1 + 2^-100 is representable, so the sum is exact.
    let small = LongDouble128::from_bits(0x3f8f_0000_0000_0000_0000_0000_0000_0000);
    let sum = one.add(small);
    assert!(sum.to_f64() == 1.0);
    assert!(sum.total_cmp(one) == core::cmp::Ordering::Greater);
    // Ordering and layout.
    assert!(
        LongDouble128::zero(true).total_cmp(LongDouble128::zero(false))
            == core::cmp::Ordering::Less
    );
    assert_eq!(LongDouble128::one().to_bits(), ONE);
    assert_eq!(LongDouble128::one().exponent_field(), 0x3fff);
    assert_eq!(LongDouble128::one().significand_bits(), 0);
    assert_eq!(LongDouble128::nan().class(), FP_NAN);
}

#[test]
fn the_binary128_type_multiplies_correctly() {
    use super::LongDouble128;
    let one_and_a_half = LongDouble128::from_f64(1.5);
    let two_and_a_quarter = LongDouble128::from_f64(2.25);
    assert_eq!(one_and_a_half.mul(two_and_a_quarter).to_f64(), 3.375);
    assert_eq!(
        LongDouble128::from_f64(2.0)
            .mul(LongDouble128::from_f64(3.0))
            .to_f64(),
        6.0
    );
    assert_eq!(
        LongDouble128::from_f64(1.5)
            .mul(LongDouble128::from_f64(-2.0))
            .to_f64(),
        -3.0
    );
    assert!(
        LongDouble128::zero(false)
            .mul(LongDouble128::infinity(false))
            .is_nan()
    );
    assert_eq!(
        LongDouble128::zero(true)
            .mul(LongDouble128::from_f64(5.0))
            .to_bits(),
        super::SIGN
    );
    assert_eq!(
        LongDouble128::from_f64(1e300)
            .mul(LongDouble128::from_f64(1e300))
            .to_f64(),
        f64::INFINITY
    );
    // (1 + 2^-52)^2 = 1 + 2^-51 + 2^-104: the last term is below the 113-bit significand.
    let near_one = LongDouble128::from_bits(0x3fff_0000_0000_0000_0000_1000_0000_0000);
    let square = near_one.mul(near_one);
    let expected = LongDouble128::from_bits(0x3fff_0000_0000_0000_0000_2000_0000_0000);
    assert_eq!(square.to_bits(), expected.to_bits());
    assert!(
        one_and_a_half.mul(one_and_a_half).total_cmp(one_and_a_half)
            == core::cmp::Ordering::Greater
    );
}

#[test]
fn the_binary128_helpers_serve_the_f128_series() {
    use super::LongDouble128;
    let value = LongDouble128::from_f64(-2.5);
    assert_eq!(value.abs().to_f64(), 2.5);
    assert_eq!(value.copysign(LongDouble128::one()).to_f64(), 2.5);
    assert_eq!(LongDouble128::one().copysign(value).to_f64(), -1.0);
    assert_eq!(LongDouble128::from_i64(-7).to_f64(), -7.0);
    assert_eq!(LongDouble128::from_i64(1 << 60).to_i64(), Some(1 << 60));
    assert_eq!(LongDouble128::from_i64(-7).to_i64(), Some(-7));
    assert_eq!(LongDouble128::from_i64(0).to_i64(), Some(0));
    assert_eq!(LongDouble128::from_f64(0.5).to_i64(), Some(0));
    assert_eq!(LongDouble128::from_f64(2.5).round().to_f64(), 3.0);
    assert_eq!(LongDouble128::from_f64(-2.5).round().to_f64(), -3.0);
    assert_eq!(LongDouble128::from_f64(0.4).round().to_f64(), 0.0);
    assert_eq!(LongDouble128::from_f64(1.5).scalbn(3).to_f64(), 12.0);
    assert_eq!(LongDouble128::from_f64(1.0).scalbn(-2).to_f64(), 0.25);
    assert_eq!(LongDouble128::one().mantissa().to_f64(), 1.0);
    assert_eq!(LongDouble128::from_f64(6.0).mantissa().to_f64(), 1.5);
    // The un-gated series module builds on these helpers on every target.
    assert!(LongDouble128::from_f64(2.0).sqrt().to_f64() > 1.41);
}

#[test]
fn the_binary128_type_divides_and_takes_square_roots() {
    use super::LongDouble128;
    let six = LongDouble128::from_f64(6.0);
    let three = LongDouble128::from_f64(3.0);
    assert_eq!(six.div(three).to_f64(), 2.0);
    assert_eq!(
        LongDouble128::from_f64(1.5)
            .div(LongDouble128::from_f64(0.5))
            .to_f64(),
        3.0
    );
    // One third: the 113-bit value rounds to the f64 1/3.
    assert_eq!(
        LongDouble128::from_f64(1.0)
            .div(LongDouble128::from_f64(3.0))
            .to_f64(),
        1.0 / 3.0
    );
    assert_eq!(LongDouble128::from_f64(4.0).sqrt().to_f64(), 2.0);
    assert_eq!(
        LongDouble128::from_f64(2.0).sqrt().to_f64(),
        core::f64::consts::SQRT_2
    );
    assert_eq!(LongDouble128::zero(false).sqrt().to_bits(), 0);
    assert!(LongDouble128::from_f64(-1.0).sqrt().is_nan());
    assert_eq!(
        LongDouble128::infinity(false).sqrt().to_f64(),
        f64::INFINITY
    );
    // Division by zero and NaN cases.
    assert_eq!(
        LongDouble128::from_f64(1.0)
            .div(LongDouble128::zero(false))
            .to_f64(),
        f64::INFINITY
    );
    assert!(
        LongDouble128::zero(false)
            .div(LongDouble128::zero(false))
            .is_nan()
    );
    assert!(
        LongDouble128::infinity(false)
            .div(LongDouble128::infinity(false))
            .is_nan()
    );
    assert_eq!(
        LongDouble128::from_f64(1.0)
            .div(LongDouble128::infinity(false))
            .to_bits(),
        0
    );
    // Sign rules.
    assert!(
        LongDouble128::from_f64(1.0)
            .div(LongDouble128::from_f64(-2.0))
            .sign_bit()
    );
    assert_eq!(
        LongDouble128::from_f64(1.0)
            .div(LongDouble128::from_f64(-2.0))
            .to_f64(),
        -0.5
    );
}

#[test]
fn the_binary128_integral_rounding_family() {
    use super::LongDouble128;
    assert_eq!(LongDouble128::from_f64(2.5).floor().to_f64(), 2.0);
    assert_eq!(LongDouble128::from_f64(2.5).ceil().to_f64(), 3.0);
    assert_eq!(LongDouble128::from_f64(-2.5).floor().to_f64(), -3.0);
    assert_eq!(LongDouble128::from_f64(-2.5).ceil().to_f64(), -2.0);
    assert_eq!(LongDouble128::from_f64(-2.5).trunc().to_f64(), -2.0);
    assert_eq!(LongDouble128::from_f64(2.5).roundeven().to_f64(), 2.0);
    assert_eq!(LongDouble128::from_f64(3.5).roundeven().to_f64(), 4.0);
    assert_eq!(LongDouble128::from_f64(-2.5).roundeven().to_f64(), -2.0);
    assert_eq!(LongDouble128::from_f64(0.4).floor().to_f64(), 0.0);
    assert_eq!(LongDouble128::from_f64(0.4).ceil().to_f64(), 1.0);
    assert_eq!(LongDouble128::from_f64(-0.4).floor().to_f64(), -1.0);
    assert_eq!(LongDouble128::from_f64(-0.4).trunc().to_f64(), -0.0);
    assert_eq!(LongDouble128::from_f64(7.0).floor().to_f64(), 7.0);
    assert_eq!(
        LongDouble128::infinity(false).floor().to_f64(),
        f64::INFINITY
    );
}

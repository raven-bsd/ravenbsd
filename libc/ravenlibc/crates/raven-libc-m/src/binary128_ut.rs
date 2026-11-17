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

#[test]
fn the_binary128_exact_class_helpers() {
    use super::LongDouble128;
    assert!(LongDouble128::from_f64(1.0).is_normal());
    assert!(!LongDouble128::zero(false).is_normal());
    assert_eq!(
        LongDouble128::from_f64(1.0).partial_cmp(LongDouble128::from_f64(2.0)),
        Some(core::cmp::Ordering::Less)
    );
    assert_eq!(
        LongDouble128::from_f64(1.0).partial_cmp(LongDouble128::nan()),
        None
    );
    assert_eq!(
        LongDouble128::from_f64(5.5)
            .fmod(LongDouble128::from_f64(2.0))
            .to_f64(),
        1.5
    );
    assert_eq!(
        LongDouble128::from_f64(-5.5)
            .fmod(LongDouble128::from_f64(2.0))
            .to_f64(),
        -1.5
    );
    assert!(
        LongDouble128::from_f64(1.0)
            .fmod(LongDouble128::zero(false))
            .is_nan()
    );
    assert_eq!(
        LongDouble128::from_f64(5.5)
            .remainder(LongDouble128::from_f64(2.0))
            .to_f64(),
        -0.5
    );
    let (value, quotient) = LongDouble128::from_f64(5.5).remquo(LongDouble128::from_f64(2.0));
    assert_eq!(value.to_f64(), -0.5);
    assert_eq!(quotient, 3);
    assert_eq!(
        LongDouble128::from_f64(5.0)
            .fdim(LongDouble128::from_f64(2.0))
            .to_f64(),
        3.0
    );
    assert_eq!(
        LongDouble128::from_f64(2.0)
            .fdim(LongDouble128::from_f64(5.0))
            .to_f64(),
        0.0
    );
    assert_eq!(
        LongDouble128::from_f64(1.0)
            .fmax(LongDouble128::from_f64(2.0))
            .to_f64(),
        2.0
    );
    assert_eq!(
        LongDouble128::from_f64(1.0)
            .fmin(LongDouble128::from_f64(2.0))
            .to_f64(),
        1.0
    );
    assert!(
        !LongDouble128::zero(true)
            .fmax(LongDouble128::zero(false))
            .sign_bit()
    );
    assert!(
        LongDouble128::zero(false)
            .fmin(LongDouble128::zero(true))
            .sign_bit()
    );
    assert_eq!(
        LongDouble128::from_f64(1.0)
            .next_after(LongDouble128::from_f64(2.0))
            .to_bits(),
        LongDouble128::from_f64(1.0).to_bits() + 1
    );
    assert_eq!(
        LongDouble128::from_f64(1.0)
            .next_after(LongDouble128::from_f64(2.0))
            .next_after(LongDouble128::from_f64(1.0))
            .to_f64(),
        1.0
    );
}

#[test]
fn the_dtoa_byte_image_is_the_binary128_pattern() {
    let bits = ONE | 0x1234;
    let bytes = LongDouble128::from_bits(bits).to_dtoa_bytes();
    assert_eq!(u128::from_le_bytes(bytes), bits);
    assert_eq!(LongDouble128::from_dtoa_bytes(bytes).to_bits(), bits);
    assert_eq!(LongDouble128::from_dtoa_bytes([0; 16]).to_bits(), 0);
    assert!(
        LongDouble128::from_dtoa_bytes(LongDouble128::from_f64(1.5).to_dtoa_bytes()).to_f64()
            == 1.5
    );
}

#[test]
fn frexp_splits_into_a_half_open_mantissa_and_exponent() {
    for &(value, want_mantissa, want_exponent) in &[
        (1.0, 0.5, 1),
        (8.0, 0.5, 4),
        (-8.0, -0.5, 4),
        (0.5, 0.5, 0),
        (0.75, 0.75, 0),
        (3.0, 0.75, 2),
    ] {
        let (mantissa, exponent) = LongDouble128::from_f64(value).frexp();
        assert_eq!(mantissa.to_f64(), want_mantissa, "frexp({value}) mantissa");
        assert_eq!(exponent, want_exponent, "frexp({value}) exponent");
    }
    let (mantissa, exponent) = LongDouble128::from_f64(0.0).frexp();
    assert!(mantissa.is_zero());
    assert_eq!(exponent, 0);
    let (mantissa, exponent) = LongDouble128::from_f64(f64::INFINITY).frexp();
    assert!(mantissa.is_infinite());
    assert_eq!(exponent, 0);
    // A value built from the smallest f64 subnormal is normal in binary128: 5e-324 = 0.5 * 2^-1073.
    let (mantissa, exponent) = LongDouble128::from_f64(f64::from_bits(1)).frexp();
    assert_eq!(mantissa.to_f64(), 0.5);
    assert_eq!(exponent, -1073);
}

#[test]
fn ilogb_and_logb_report_the_unbiased_exponent() {
    assert_eq!(LongDouble128::from_f64(1.0).ilogb(), 0);
    assert_eq!(LongDouble128::from_f64(8.0).ilogb(), 3);
    assert_eq!(LongDouble128::from_f64(0.5).ilogb(), -1);
    assert_eq!(
        LongDouble128::from_f64(0.0).ilogb(),
        crate::consts::FP_ILOGB0
    );
    assert_eq!(
        LongDouble128::from_f64(f64::INFINITY).ilogb(),
        crate::consts::FP_ILOGBNAN
    );
    assert_eq!(
        LongDouble128::from_f64(f64::NAN).ilogb(),
        crate::consts::FP_ILOGBNAN
    );
    assert_eq!(LongDouble128::from_f64(8.0).logb().to_f64(), 3.0);
    let zero_logb = LongDouble128::from_f64(0.0).logb();
    assert!(zero_logb.is_infinite() && zero_logb.sign_bit());
    let negative_infinity_logb = LongDouble128::from_f64(f64::NEG_INFINITY).logb();
    assert!(negative_infinity_logb.is_infinite() && negative_infinity_logb.sign_bit());
    assert!(LongDouble128::from_f64(f64::NAN).logb().is_nan());
}

#[test]
fn significand_scales_into_one_to_two() {
    assert_eq!(LongDouble128::from_f64(1.0).significand().to_f64(), 1.0);
    assert_eq!(LongDouble128::from_f64(8.0).significand().to_f64(), 1.0);
    assert_eq!(LongDouble128::from_f64(0.75).significand().to_f64(), 1.5);
    assert_eq!(LongDouble128::from_f64(-0.75).significand().to_f64(), -1.5);
    assert!(LongDouble128::from_f64(0.0).significand().is_zero());
    assert!(
        LongDouble128::from_f64(f64::INFINITY)
            .significand()
            .is_infinite()
    );
}

#[test]
fn modf_returns_the_fraction_and_the_truncated_integral() {
    let (fraction, integral) = LongDouble128::from_f64(3.75).modf();
    assert_eq!(fraction.to_f64(), 0.75);
    assert_eq!(integral.to_f64(), 3.0);
    let (fraction, integral) = LongDouble128::from_f64(-3.75).modf();
    assert_eq!(fraction.to_f64(), -0.75);
    assert_eq!(integral.to_f64(), -3.0);
    let (fraction, integral) = LongDouble128::from_f64(5.0).modf();
    assert!(fraction.is_zero());
    assert_eq!(integral.to_f64(), 5.0);
    let (fraction, integral) = LongDouble128::from_f64(0.25).modf();
    assert_eq!(fraction.to_f64(), 0.25);
    assert!(integral.is_zero());
    let (fraction, integral) = LongDouble128::from_f64(f64::INFINITY).modf();
    assert!(fraction.is_zero() && integral.is_infinite());
    let (fraction, integral) = LongDouble128::from_f64(f64::NAN).modf();
    assert!(fraction.is_nan() && integral.is_nan());
}

#[test]
fn cbrt_is_within_one_ulp_of_the_f64_reference() {
    // Exact cubes and special values.
    assert_eq!(LongDouble128::from_f64(8.0).cbrt().to_f64(), 2.0);
    assert_eq!(LongDouble128::from_f64(27.0).cbrt().to_f64(), 3.0);
    assert_eq!(LongDouble128::from_f64(0.125).cbrt().to_f64(), 0.5);
    assert_eq!(LongDouble128::from_f64(-27.0).cbrt().to_f64(), -3.0);
    assert!(LongDouble128::zero(false).cbrt().is_zero());
    assert!(LongDouble128::from_f64(f64::INFINITY).cbrt().is_infinite());
    assert!(
        LongDouble128::from_f64(f64::NEG_INFINITY)
            .cbrt()
            .is_infinite()
    );
    assert!(LongDouble128::from_f64(-1.0).cbrt().to_f64() < -0.99);
    // f64 is the only in-tree reference for the binary128 result; compare its rounding.
    for &value in &[
        0.5,
        1.0,
        2.0,
        3.0,
        7.0,
        1e-9,
        1e9,
        1234.5,
        1.0e300,
        1.0e-300,
        f64::MIN_POSITIVE,
    ] {
        let got = LongDouble128::from_f64(value).cbrt().to_f64();
        let want = value.cbrt();
        assert_eq!(got, want, "cbrt({value})");
    }
}

#[test]
fn hypot_is_exact_for_f64_pythagorean_triples() {
    let hypot = |x: f64, y: f64| {
        LongDouble128::from_f64(x)
            .hypot(LongDouble128::from_f64(y))
            .to_f64()
    };
    assert_eq!(hypot(3.0, 4.0), 5.0);
    assert_eq!(hypot(5.0, 12.0), 13.0);
    assert_eq!(hypot(8.0, 15.0), 17.0);
    assert_eq!(hypot(0.0, 0.0), 0.0);
    assert_eq!(hypot(-3.0, -4.0), 5.0);
    // `hypot(x, 0)` must be exactly `|x|` (the scaled path multiplies by one).
    for &value in &[1.0, 2.5, 1.0e300, 1.0e-300, f64::MIN_POSITIVE] {
        assert_eq!(
            LongDouble128::from_f64(value)
                .hypot(LongDouble128::zero(false))
                .to_f64(),
            value.abs()
        );
        assert_eq!(
            LongDouble128::from_f64(value)
                .hypot(LongDouble128::zero(false))
                .to_bits(),
            LongDouble128::from_f64(value).abs().to_bits()
        );
    }
    assert!(
        LongDouble128::from_f64(f64::INFINITY)
            .hypot(LongDouble128::from_f64(f64::NAN))
            .is_infinite()
    );
    assert!(
        LongDouble128::from_f64(1.0)
            .hypot(LongDouble128::from_f64(f64::NAN))
            .is_nan()
    );
    // The scaling keeps huge and tiny pairs finite.
    assert_eq!(hypot(1.0e300, 1.0e300), 1.4142135623730951e300);
    assert_eq!(hypot(1.0e-300, 1.0e-300), 1.4142135623730951e-300);
}

#[test]
fn subtraction_keeps_the_cancelled_result_scale() {
    // `pack_binary128` must renormalise a cancelled significand and adjust the scale with it.
    // These pairs cancelled to a result narrower than 113 bits and used to come back as 0.25.
    let two = LongDouble128::from_f64(2.0);
    for &(value, expected) in &[
        (2.0625, -0.0625),
        (2.125, -0.125),
        (2.0009765625, -0.0009765625),
        (3.0, -1.0),
        (2.25, -0.25),
        (2.5, -0.5),
        (2.0000000000000004, -4.440892098500626e-16),
    ] {
        assert_eq!(
            two.sub(LongDouble128::from_f64(value)).to_f64(),
            expected,
            "2 - {value}"
        );
        assert_eq!(
            LongDouble128::from_f64(value).sub(two).to_f64(),
            -expected,
            "{value} - 2"
        );
    }
    // A subtraction that cancels to exactly one ULP of the larger operand.
    let one = LongDouble128::one();
    let next = one.next_after(LongDouble128::from_f64(2.0));
    assert_eq!(next.sub(one).to_f64(), 2f64.powi(-112));
}

#[test]
fn exp_and_log_edges_and_exact_identities() {
    // exp: NaN, ±inf, ±0.
    assert!(LongDouble128::from_f64(f64::NAN).exp().is_nan());
    assert_eq!(
        LongDouble128::from_f64(f64::INFINITY).exp().to_f64(),
        f64::INFINITY
    );
    assert!(LongDouble128::from_f64(f64::NEG_INFINITY).exp().is_zero());
    assert_eq!(
        LongDouble128::from_f64(0.0).exp().to_bits(),
        LongDouble128::one().to_bits()
    );
    assert_eq!(
        LongDouble128::from_f64(-0.0).exp().to_bits(),
        LongDouble128::one().to_bits()
    );
    // log: NaN, ±0, negatives, +inf, 1.
    assert!(LongDouble128::from_f64(f64::NAN).ln().is_nan());
    let log_zero = LongDouble128::from_f64(0.0).ln();
    assert!(log_zero.is_infinite() && log_zero.sign_bit());
    assert!(LongDouble128::from_f64(-1.0).ln().is_nan());
    assert_eq!(
        LongDouble128::from_f64(f64::INFINITY).ln().to_f64(),
        f64::INFINITY
    );
    assert!(LongDouble128::one().ln().is_zero());
    // exp2 / log2 / log10 identities.
    assert_eq!(
        LongDouble128::from_f64(0.0).exp2().to_bits(),
        LongDouble128::one().to_bits()
    );
    assert!(LongDouble128::from_f64(f64::NEG_INFINITY).exp2().is_zero());
    assert!(LongDouble128::from_f64(f64::INFINITY).exp2().is_infinite());
    for k in -40i64..=40 {
        let value = LongDouble128::from_i64(k);
        // exp2(k) is an exact power of two constructed through the series.
        let got = value.exp2();
        assert_eq!(got.to_f64(), 2f64.powi(i32::try_from(k).unwrap_or(0)));
        // log2 of that power of two is the exact integer.
        assert_eq!(got.log2().to_f64(), k as f64);
    }
    assert!(LongDouble128::from_f64(-2.0).log2().is_nan());
    assert_eq!(
        LongDouble128::from_f64(100.0).log10().to_f64(),
        100.0f64.log10()
    );
}

/// ULP distance between two `f64` values; negatives compare by magnitude, signs differ → far.
fn f64_ulp(a: f64, b: f64) -> u64 {
    if a == b {
        return 0;
    }
    if a.is_nan() || b.is_nan() || a.is_sign_negative() != b.is_sign_negative() {
        return u64::MAX;
    }
    a.abs().to_bits().abs_diff(b.abs().to_bits())
}

#[test]
fn exp_and_log_match_the_f64_oracle_over_a_deterministic_sweep() {
    let mut state: u64 = 0x2545_f491_4f6c_dd1d;
    let mut checked = 0u32;
    while checked < 2_000 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let x = f64::from_bits(state);
        if !x.is_finite() || x == 0.0 {
            continue;
        }
        // log of positive values only.
        let positive = LongDouble128::from_f64(x.abs());
        assert_eq!(positive.ln().to_f64(), x.abs().ln(), "ln({})", x.abs());
        // `log2`/`log10` are `ln` divided by a constant, so allow two f64 ULP.
        assert!(
            f64_ulp(positive.log2().to_f64(), x.abs().log2()) <= 2,
            "log2({})",
            x.abs()
        );
        assert!(
            f64_ulp(positive.log10().to_f64(), x.abs().log10()) <= 2,
            "log10({})",
            x.abs()
        );
        // exp of a moderate argument (the binary128 exp range is much wider than f64's).
        let small = x % 100.0;
        assert_eq!(
            LongDouble128::from_f64(small).exp().to_f64(),
            small.exp(),
            "exp({small})"
        );
        assert!(
            f64_ulp(
                LongDouble128::from_f64(small / 100.0).exp2().to_f64(),
                (small / 100.0).exp2()
            ) <= 1,
            "exp2({})",
            small / 100.0
        );
        checked = checked.saturating_add(1);
    }
}

#[test]
fn exp_and_log_round_trip_within_a_few_ulp_of_the_log() {
    // ULP distance on the positive binary128 line: the bit patterns are monotone there.
    fn ulp_distance(a: LongDouble128, b: LongDouble128) -> u128 {
        a.to_bits().abs_diff(b.to_bits())
    }
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut checked = 0u32;
    while checked < 500 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let x = f64::from_bits(state);
        if !x.is_finite() || x <= 0.0 {
            continue;
        }
        let value = LongDouble128::from_f64(x);
        // exp(ln(x)) carries ln's absolute error, so compare the relative error instead: the
        // 113-bit log keeps it below 2^-100 even for |ln x| in the thousands.
        let round_trip = value.ln().exp();
        let relative = round_trip.sub(value).div(value).abs();
        assert!(
            relative.to_f64() <= 2f64.powi(-100),
            "exp(ln({x})) relative error {}",
            relative.to_f64()
        );
        // ln(exp(y)) for a moderate y: the round trip is absolute-accurate to a few ULP of 1
        // (a tiny y has very few significant bits after exp rounds 1+y, so ULP-of-y is not the
        // right yardstick). Values below 2^-90 are skipped: exp(y) is exactly 1 there.
        let y = LongDouble128::from_f64(x.fract() * 40.0);
        let tiny = y.abs().to_f64() < 2f64.powi(-90);
        let back = y.exp().ln();
        if !y.is_zero() && !tiny {
            let absolute = back.sub(y).abs().to_f64();
            assert!(
                absolute <= 2f64.powi(-100),
                "ln(exp({})) absolute error is {absolute:e}",
                x.fract() * 40.0
            );
        }
        checked = checked.saturating_add(1);
    }
}

#[test]
fn exp_log_constants_and_extremes() {
    // ln2_hi + ln2_lo must exponentiate back to 2 within a few ULP.
    let ln2 = LongDouble128::ln2_hi().add(LongDouble128::ln2_lo());
    let two = LongDouble128::from_f64(2.0);
    assert!(ln2.exp().to_bits().abs_diff(two.to_bits()) <= 4);
    // ln10 rounds back to the f64 constant.
    assert_eq!(LongDouble128::ln10().to_f64(), core::f64::consts::LN_10);
    // Overflow and underflow edges of exp.
    assert!(LongDouble128::from_f64(12000.0).exp().is_infinite());
    assert!(LongDouble128::from_f64(-12000.0).exp().is_zero());
    // A subnormal input still has a finite, negative log.
    let tiny = LongDouble128::from_f64(f64::from_bits(1));
    assert!(tiny.ln().to_f64() < -700.0);
    assert!(tiny.ln().is_finite());
}

#[test]
fn cbrt_and_hypot_match_the_f64_oracle_over_a_deterministic_sweep() {
    // The only in-tree reference for binary128 is the host f64 math; the binary128 result must
    // round to the same f64 for a spread of exponents and mantissas. Deterministic LCG, no
    // dependency, so the sweep is reproducible.
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut checked = 0u32;
    while checked < 2_000 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let x = f64::from_bits(state);
        if !x.is_finite() || x == 0.0 {
            continue;
        }
        assert_eq!(
            LongDouble128::from_f64(x).cbrt().to_f64(),
            x.cbrt(),
            "cbrt({x})"
        );
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let y = f64::from_bits(state);
        if !y.is_finite() {
            continue;
        }
        assert_eq!(
            LongDouble128::from_f64(x)
                .hypot(LongDouble128::from_f64(y))
                .to_f64(),
            x.hypot(y),
            "hypot({x}, {y})"
        );
        checked = checked.saturating_add(1);
    }
}

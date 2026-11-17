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

#[test]
fn trig_edges_domain_and_sincos_consistency() {
    let zero = LongDouble128::from_f64(0.0);
    let negative_zero = LongDouble128::from_f64(-0.0);
    // sin(±0) = ±0, cos(±0) = 1, tan(±0) = ±0.
    assert_eq!(zero.sin().to_bits(), 0);
    assert_eq!(negative_zero.sin().to_bits(), SIGN);
    assert_eq!(zero.cos().to_bits(), LongDouble128::one().to_bits());
    assert_eq!(
        negative_zero.cos().to_bits(),
        LongDouble128::one().to_bits()
    );
    assert_eq!(zero.tan().to_bits(), 0);
    assert_eq!(negative_zero.tan().to_bits(), SIGN);
    // NaN and the infinite domain errors.
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let x = LongDouble128::from_f64(value);
        assert!(x.sin().is_nan(), "sin({value})");
        assert!(x.cos().is_nan(), "cos({value})");
        assert!(x.tan().is_nan(), "tan({value})");
        assert!(x.asin().is_nan(), "asin({value})");
        assert!(x.acos().is_nan(), "acos({value})");
        assert!(x.atanh().is_nan(), "atanh({value})");
        let (sine, cosine) = x.sincos();
        assert!(sine.is_nan() && cosine.is_nan(), "sincos({value})");
    }
    // atan(±inf) = ±π/2.
    let half_pi = LongDouble128::pio2_hi().add(LongDouble128::pio2_lo());
    let atan_inf = LongDouble128::from_f64(f64::INFINITY).atan();
    assert_eq!(atan_inf.to_bits(), half_pi.to_bits());
    assert_eq!(
        LongDouble128::from_f64(f64::NEG_INFINITY).atan().to_bits(),
        half_pi.to_bits() | SIGN
    );
    // Reduction-domain edge: the quadrant index fits an i64 below ~1.45e19.
    let inside = LongDouble128::from_f64(2f64.powi(62));
    let sine = inside.sin();
    assert!(sine.is_finite() && sine.abs().to_f64() <= 1.0);
    assert!(inside.cos().is_finite());
    let outside = LongDouble128::from_f64(2.0e19);
    assert!(outside.sin().is_nan());
    assert!(outside.cos().is_nan());
    assert!(outside.tan().is_nan());
    // Inverse-trig domains.
    assert!(LongDouble128::from_f64(1.5).asin().is_nan());
    assert!(LongDouble128::from_f64(-1.5).acos().is_nan());
    assert_eq!(
        LongDouble128::from_f64(1.0).asin().to_bits(),
        half_pi.to_bits()
    );
    assert_eq!(LongDouble128::from_f64(1.0).acos().to_bits(), 0);
    assert!(
        f64_ulp(
            LongDouble128::from_f64(-1.0).acos().to_f64(),
            core::f64::consts::PI
        ) <= 1
    );
    // atan2 edge table (C23 F.10.1.4): zeros, infinities and the quadrants.
    let one = LongDouble128::one();
    assert_eq!(zero.atan2(one).to_bits(), 0);
    assert_eq!(negative_zero.atan2(one).to_bits(), SIGN);
    assert!(
        f64_ulp(
            zero.atan2(LongDouble128::from_f64(-1.0)).to_f64(),
            core::f64::consts::PI
        ) <= 1
    );
    assert!(
        f64_ulp(
            negative_zero.atan2(LongDouble128::from_f64(-1.0)).to_f64(),
            -core::f64::consts::PI
        ) <= 1
    );
    assert_eq!(one.atan2(zero).to_bits(), half_pi.to_bits());
    assert_eq!(
        LongDouble128::zero(false)
            .sub(LongDouble128::one())
            .atan2(zero)
            .to_bits(),
        half_pi.to_bits() | SIGN
    );
    assert!(
        f64_ulp(
            LongDouble128::from_f64(f64::INFINITY)
                .atan2(LongDouble128::from_f64(f64::NEG_INFINITY))
                .to_f64(),
            3.0 * core::f64::consts::FRAC_PI_4
        ) <= 2
    );
    // sincos agrees with the separate calls and the Pythagorean identity holds.
    for &value in &[0.3f64, -1.0, 2.5, 12.0, 1.0e6, 4.0e18] {
        let x = LongDouble128::from_f64(value);
        let (sine, cosine) = x.sincos();
        assert_eq!(sine.to_bits(), x.sin().to_bits(), "sincos sin({value})");
        assert_eq!(cosine.to_bits(), x.cos().to_bits(), "sincos cos({value})");
        assert_eq!(
            x.tan().to_bits(),
            sine.div(cosine).to_bits(),
            "tan({value})"
        );
        let norm = sine.mul(sine).add(cosine.mul(cosine));
        assert!(
            norm.to_bits().abs_diff(LongDouble128::one().to_bits()) <= 8,
            "sin^2+cos^2 at {value}"
        );
    }
}

#[test]
fn hyperbolic_edges_and_identities() {
    let zero = LongDouble128::from_f64(0.0);
    let negative_zero = LongDouble128::from_f64(-0.0);
    assert_eq!(zero.sinh().to_bits(), 0);
    assert_eq!(negative_zero.sinh().to_bits(), SIGN);
    assert_eq!(zero.cosh().to_bits(), LongDouble128::one().to_bits());
    assert_eq!(
        negative_zero.cosh().to_bits(),
        LongDouble128::one().to_bits()
    );
    assert_eq!(zero.tanh().to_bits(), 0);
    assert_eq!(negative_zero.tanh().to_bits(), SIGN);
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let x = LongDouble128::from_f64(value);
        assert!(x.sinh().is_nan() || x.sinh().is_infinite());
        assert!(x.cosh().is_infinite() || x.cosh().is_nan());
        assert!(x.asinh().is_infinite() || x.asinh().is_nan());
        assert!(x.acosh().is_nan() || x.acosh().is_infinite());
    }
    assert_eq!(
        LongDouble128::from_f64(f64::INFINITY).sinh().to_f64(),
        f64::INFINITY
    );
    assert_eq!(
        LongDouble128::from_f64(f64::NEG_INFINITY).sinh().to_f64(),
        f64::NEG_INFINITY
    );
    assert_eq!(
        LongDouble128::from_f64(f64::NEG_INFINITY).cosh().to_f64(),
        f64::INFINITY
    );
    assert_eq!(LongDouble128::from_f64(f64::INFINITY).tanh().to_f64(), 1.0);
    assert_eq!(
        LongDouble128::from_f64(f64::NEG_INFINITY).tanh().to_f64(),
        -1.0
    );
    // The true sinh/cosh overflow edge: exp overflows first, the halves must survive.
    let large = LongDouble128::from_f64(11357.0);
    assert!(large.exp().is_infinite(), "exp already overflows here");
    assert!(large.sinh().is_finite(), "sinh(11357) must still be finite");
    assert!(large.cosh().is_finite(), "cosh(11357) must still be finite");
    assert!(LongDouble128::from_f64(12000.0).sinh().is_infinite());
    assert_eq!(LongDouble128::from_f64(12000.0).tanh().to_f64(), 1.0);
    // Inverse-hyperbolic domains.
    assert_eq!(LongDouble128::from_f64(1.0).acosh().to_bits(), 0);
    assert!(LongDouble128::from_f64(0.5).acosh().is_nan());
    assert!(LongDouble128::from_f64(1.25).atanh().is_nan());
    assert_eq!(LongDouble128::from_f64(1.0).atanh().to_f64(), f64::INFINITY);
    assert_eq!(
        LongDouble128::from_f64(-1.0).atanh().to_f64(),
        f64::NEG_INFINITY
    );
    // asinh/acosh stay finite far outside the f64 range.
    let huge = LongDouble128::from_f64(1.0e300);
    assert!(huge.asinh().is_finite());
    assert!(huge.acosh().is_finite());
    assert!(f64_ulp(huge.asinh().to_f64(), 1.0e300f64.asinh()) <= 1);
    // cosh^2 - sinh^2 = 1 at small arguments (larger ones lose the 1 to cancellation in the
    // identity itself, not in the kernels).
    for &value in &[0.25f64, 1.0] {
        let x = LongDouble128::from_f64(value);
        let difference = x.cosh().mul(x.cosh()).sub(x.sinh().mul(x.sinh()));
        assert!(
            difference
                .to_bits()
                .abs_diff(LongDouble128::one().to_bits())
                <= 16,
            "cosh^2-sinh^2 at {value}"
        );
    }
}

#[test]
fn trig_and_inverse_match_the_f64_oracle_over_a_deterministic_sweep() {
    let mut state: u64 = 0x0123_4567_89ab_cdef;
    let mut checked = 0u32;
    while checked < 2_000 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let raw = f64::from_bits(state);
        if !raw.is_finite() || raw == 0.0 {
            continue;
        }
        // Moderate arguments: the binary128 result must round to the same f64.
        let x = raw % 1.0e6;
        let value = LongDouble128::from_f64(x);
        assert!(f64_ulp(value.sin().to_f64(), x.sin()) <= 2, "sin({x})");
        assert!(f64_ulp(value.cos().to_f64(), x.cos()) <= 2, "cos({x})");
        if x.cos().abs() > 1e-4 {
            assert!(f64_ulp(value.tan().to_f64(), x.tan()) <= 4, "tan({x})");
        }
        // Large arguments exercise the four-part reduction.
        let large = raw % 4.0e18;
        let big = LongDouble128::from_f64(large);
        assert!(
            f64_ulp(big.sin().to_f64(), large.sin()) <= 2,
            "sin({large})"
        );
        assert!(
            f64_ulp(big.cos().to_f64(), large.cos()) <= 2,
            "cos({large})"
        );
        // atan over the whole line, atan2 in the plane.
        assert!(f64_ulp(value.atan().to_f64(), x.atan()) <= 2, "atan({x})");
        assert!(
            f64_ulp(big.atan().to_f64(), large.atan()) <= 2,
            "atan({large})"
        );
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let other = f64::from_bits(state) % 1.0e6;
        if other.is_finite() && other != 0.0 {
            assert!(
                f64_ulp(
                    LongDouble128::from_f64(x)
                        .atan2(LongDouble128::from_f64(other))
                        .to_f64(),
                    x.atan2(other)
                ) <= 2,
                "atan2({x}, {other})"
            );
        }
        // asin/acos on [-1, 1].
        let unit = x % 1.0;
        let unit_value = LongDouble128::from_f64(unit);
        assert!(
            f64_ulp(unit_value.asin().to_f64(), unit.asin()) <= 2,
            "asin({unit})"
        );
        assert!(
            f64_ulp(unit_value.acos().to_f64(), unit.acos()) <= 2,
            "acos({unit})"
        );
        checked = checked.saturating_add(1);
    }
}

#[test]
fn hyperbolic_match_the_f64_oracle_over_a_deterministic_sweep() {
    let mut state: u64 = 0xdead_beef_0bad_f00d;
    let mut checked = 0u32;
    while checked < 2_000 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let raw = f64::from_bits(state);
        if !raw.is_finite() || raw == 0.0 {
            continue;
        }
        let x = raw % 700.0;
        let value = LongDouble128::from_f64(x);
        assert!(f64_ulp(value.sinh().to_f64(), x.sinh()) <= 4, "sinh({x})");
        assert!(f64_ulp(value.cosh().to_f64(), x.cosh()) <= 4, "cosh({x})");
        assert!(f64_ulp(value.tanh().to_f64(), x.tanh()) <= 4, "tanh({x})");
        // asinh everywhere, acosh on [1, inf), atanh on (-1, 1). The oracle is the composed
        // `f64` identity rather than `f64::asinh`/`acosh`/`atanh`, whose last-place accuracy in
        // the running toolchain is not reliable enough for a ULP assertion.
        let wide = raw % 1.0e6;
        assert!(
            f64_ulp(LongDouble128::from_f64(wide).asinh().to_f64(), wide.asinh()) <= 4,
            "asinh({wide})"
        );
        let above = 1.0 + (raw % 1.0e6).abs();
        assert!(
            f64_ulp(
                LongDouble128::from_f64(above).acosh().to_f64(),
                above.acosh()
            ) <= 4,
            "acosh({above})"
        );
        // `f64::atanh` in the running toolchain loses several ULP near +-1, so the oracle is
        // the series below 0.125 and the composed logarithm above it.
        let small = (raw % 0.99) / 1.01;
        let atanh_oracle = if small.abs() <= 0.125 {
            let x2 = small * small;
            let mut term = small;
            let mut sum = small;
            for k in 1..=9u32 {
                term *= x2;
                sum += term / f64::from(2 * k + 1);
            }
            sum
        } else {
            0.5 * ((1.0 + small) / (1.0 - small)).ln()
        };
        assert!(
            f64_ulp(
                LongDouble128::from_f64(small).atanh().to_f64(),
                atanh_oracle
            ) <= 4,
            "atanh({small})"
        );
        checked = checked.saturating_add(1);
    }
}

#[test]
fn subtraction_keeps_the_cancelled_bits_across_a_one_bit_exponent_gap() {
    // Regression: the alignment in `add_bits` used to fold the shifted-out bits into a whole
    // guard unit with only three guard bits, so `(1 - 2^-113) - 1` lost ~5 decimal digits.
    let one = LongDouble128::one();
    for k in 1..=12u32 {
        let delta = one.scalbn(-(k as i32));
        let below = one.sub(delta);
        assert_eq!(
            below.sub(one).to_bits(),
            LongDouble128::zero(false).sub(delta).to_bits(),
            "1 - 2^-{k}"
        );
        assert_eq!(
            below.sub(one).to_f64(),
            -2f64.powi(-(k as i32)),
            "1 - 2^-{k} value"
        );
    }
}

#[test]
fn erf_and_erfc_edges_identities_and_known_values() {
    let zero = LongDouble128::from_f64(0.0);
    assert_eq!(zero.erf().to_bits(), 0);
    assert_eq!(LongDouble128::from_f64(-0.0).erf().to_bits(), SIGN);
    assert_eq!(zero.erfc().to_bits(), LongDouble128::one().to_bits());
    assert_eq!(LongDouble128::from_f64(f64::INFINITY).erf().to_f64(), 1.0);
    assert_eq!(
        LongDouble128::from_f64(f64::NEG_INFINITY).erf().to_f64(),
        -1.0
    );
    assert_eq!(LongDouble128::from_f64(f64::INFINITY).erfc().to_bits(), 0);
    assert_eq!(
        LongDouble128::from_f64(f64::NEG_INFINITY).erfc().to_f64(),
        2.0
    );
    assert!(LongDouble128::from_f64(f64::NAN).erf().is_nan());
    assert!(LongDouble128::from_f64(f64::NAN).erfc().is_nan());
    // Known values.
    let at_one = LongDouble128::from_f64(1.0);
    assert!(f64_ulp(at_one.erf().to_f64(), 0.8427007929497149) <= 2);
    assert!(f64_ulp(at_one.erfc().to_f64(), 0.15729920705028513) <= 2);
    // Odd symmetry and erf + erfc = 1.
    for &value in &[0.1f64, 0.25, 0.6, 1.0, 2.5, 5.0, 12.0] {
        let x = LongDouble128::from_f64(value);
        assert_eq!(
            LongDouble128::from_f64(-value).erf().to_bits(),
            LongDouble128::zero(false).sub(x.erf()).to_bits(),
            "erf(-{value})"
        );
        let sum = x.erf().add(x.erfc());
        assert!(
            sum.to_bits().abs_diff(LongDouble128::one().to_bits()) <= 8,
            "erf+erfc at {value}"
        );
        // erfc(x) and 2 - erfc(-x) are the same function.
        let difference = LongDouble128::from_f64(2.0)
            .sub(LongDouble128::from_f64(-value).erfc())
            .sub(x.erfc());
        assert!(difference.abs().to_f64() < 1e-15);
    }
    // The tail: erfc stays positive while representable, then underflows cleanly.
    let far = LongDouble128::from_f64(40.0);
    assert!(far.erfc().is_finite() && !far.erfc().is_zero());
    assert!(far.erfc().to_f64() < 1e-300);
    assert_eq!(LongDouble128::from_f64(120.0).erfc().to_bits(), 0);
    assert_eq!(LongDouble128::from_f64(1.0e10).erfc().to_bits(), 0);
    assert_eq!(LongDouble128::from_f64(1.0e10).erf().to_f64(), 1.0);
}

#[test]
fn lgamma_and_gamma_edges_and_functional_equations() {
    let one = LongDouble128::one();
    let two = LongDouble128::from_f64(2.0);
    assert_eq!(one.lgamma().to_bits(), 0);
    assert_eq!(two.lgamma().to_bits(), 0);
    for pole in [0.0, -0.0, -1.0, -2.0, -3.0] {
        let value = LongDouble128::from_f64(pole).lgamma();
        assert!(value.is_infinite() && !value.sign_bit(), "lgamma({pole})");
    }
    assert!(
        LongDouble128::from_f64(f64::INFINITY)
            .lgamma()
            .is_infinite()
    );
    assert!(
        LongDouble128::from_f64(f64::NEG_INFINITY)
            .lgamma()
            .is_infinite()
    );
    assert!(LongDouble128::from_f64(f64::NAN).lgamma().is_nan());
    // lgamma(1/2) = ln(sqrt(pi)).
    let half = LongDouble128::from_f64(0.5);
    let expected = 0.5 * core::f64::consts::PI.ln();
    assert!(f64_ulp(half.lgamma().to_f64(), expected) <= 4);
    // The functional equation lgamma(x+1) = lgamma(x) + ln(x).
    for &value in &[0.75f64, 0.9, 1.1, 1.5, 2.5, 5.0, 10.0, 100.0] {
        let x = LongDouble128::from_f64(value);
        let left = x.add(one).lgamma().sub(x.lgamma());
        assert!(
            f64_ulp(left.to_f64(), value.ln()) <= 32,
            "lgamma({value}+1)-lgamma({value})"
        );
    }
    // The reflection sign: Gamma(-1/2) < 0, Gamma(-3/2) > 0, Gamma(-5/2) < 0.
    assert_eq!(LongDouble128::from_f64(-0.5).lgamma_r().1, -1);
    assert_eq!(LongDouble128::from_f64(-1.5).lgamma_r().1, 1);
    assert_eq!(LongDouble128::from_f64(-2.5).lgamma_r().1, -1);
    assert_eq!(LongDouble128::from_f64(2.5).lgamma_r().1, 1);
    // Gamma edges.
    assert_eq!(one.gamma().to_f64(), 1.0);
    assert_eq!(two.gamma().to_f64(), 1.0);
    assert_eq!(
        LongDouble128::from_f64(6.0).gamma().to_bits(),
        LongDouble128::from_f64(120.0).to_bits()
    );
    assert_eq!(
        LongDouble128::from_f64(10.0).gamma().to_bits(),
        LongDouble128::from_f64(362_880.0).to_bits()
    );
    assert!(f64_ulp(half.gamma().to_f64(), core::f64::consts::PI.sqrt()) <= 4);
    assert!(
        f64_ulp(
            LongDouble128::from_f64(1.5).gamma().to_f64(),
            0.5 * core::f64::consts::PI.sqrt()
        ) <= 4
    );
    assert!(
        f64_ulp(
            LongDouble128::from_f64(-0.5).gamma().to_f64(),
            -2.0 * core::f64::consts::PI.sqrt()
        ) <= 8
    );
    // Poles and domains.
    let positive_zero_inf = LongDouble128::from_f64(0.0).gamma();
    assert!(positive_zero_inf.is_infinite() && !positive_zero_inf.sign_bit());
    let negative_zero_inf = LongDouble128::from_f64(-0.0).gamma();
    assert!(negative_zero_inf.is_infinite() && negative_zero_inf.sign_bit());
    assert!(LongDouble128::from_f64(-1.0).gamma().is_nan());
    assert!(LongDouble128::from_f64(-2.0).gamma().is_nan());
    assert_eq!(
        LongDouble128::from_f64(f64::INFINITY).gamma().to_f64(),
        f64::INFINITY
    );
    assert!(LongDouble128::from_f64(f64::NEG_INFINITY).gamma().is_nan());
    assert!(LongDouble128::from_f64(f64::NAN).gamma().is_nan());
    // Gamma(x+1) = x*Gamma(x) away from the poles.
    for &value in &[0.75f64, 1.3, 2.5, 5.0, 42.0] {
        let x = LongDouble128::from_f64(value);
        let left = x.add(one).gamma();
        let right = x.mul(x.gamma());
        let relative = left.sub(right).div(right).abs().to_f64();
        assert!(
            relative <= 1e-30,
            "Gamma recurrence at {value}: {relative:e}"
        );
    }
    // Reflection: Gamma(x)*Gamma(1-x) = pi/sin(pi x).
    for &value in &[-0.3f64, -1.5, -2.5, -5.5] {
        let x = LongDouble128::from_f64(value);
        let product = x.gamma().mul(LongDouble128::one().sub(x).gamma());
        let sine = core::f64::consts::PI * value;
        assert!(
            f64_ulp(product.to_f64(), core::f64::consts::PI / sine.sin()) <= 256,
            "reflection at {value}"
        );
    }
}

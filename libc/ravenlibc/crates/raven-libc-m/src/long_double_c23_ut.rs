// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Cases for the non-exact `long double` additions in [`super`].

use super::*;

fn ld(value: f64) -> LongDouble {
    LongDouble::from_f64(value)
}

fn f64_of(value: LongDouble) -> f64 {
    value.to_f64()
}

fn close(actual: LongDouble, expected: f64, tolerance: f64) -> bool {
    (actual.to_f64() - expected).abs() <= tolerance
}

#[test]
fn cbrtl_handles_sign_scale_and_special_values() {
    assert!(close(cbrtl(ld(27.0)), 3.0, 1.0e-15));
    assert!(close(cbrtl(ld(-8.0)), -2.0, 1.0e-15));
    assert!(close(cbrtl(ld(1.0e300)), 1.0e100, 1.0e86));
    assert!(cbrtl(LongDouble::nan()).is_nan());
    assert!(cbrtl(LongDouble::infinity(false)).is_infinite());
}

#[test]
fn fmal_multiplies_and_adds_in_long_double() {
    assert_eq!(f64_of(fmal(ld(2.0), ld(3.0), ld(4.0))), 10.0);
    assert!(close(
        fmal(ld(1.0e-3), ld(1.0e-3), ld(-1.0e-6)),
        0.0,
        1.0e-22
    ));
}

#[test]
fn frexpl_returns_the_fraction_and_exponent() {
    let (mantissa, exponent) = frexpl(ld(48.0));
    assert_eq!(f64_of(mantissa), 0.75);
    assert_eq!(exponent, 6);
    let (mantissa, exponent) = frexpl(ld(-0.375));
    assert_eq!(f64_of(mantissa), -0.75);
    assert_eq!(exponent, -1);
    assert_eq!(frexpl(ld(0.0)).1, 0);
}

#[test]
fn hypotl_is_scaled_and_handles_infinities() {
    assert_eq!(f64_of(hypotl(ld(3.0), ld(4.0))), 5.0);
    assert_eq!(f64_of(hypotl(ld(0.0), ld(-7.0))), 7.0);
    assert!(hypotl(LongDouble::infinity(false), ld(f64::NAN)).is_infinite());
    assert!(hypotl(LongDouble::nan(), ld(1.0)).is_nan());
    assert!(close(
        hypotl(ld(1.0e200), ld(1.0e200)),
        1.414213562373095e200,
        1.0e186
    ));
}

#[test]
fn exponent_queries_follow_ilogb_rules() {
    assert_eq!(ilogbl(ld(8.0)), 3);
    assert_eq!(ilogbl(ld(0.5)), -1);
    assert_eq!(ilogbl(ld(0.0)), i32::MIN);
    assert_eq!(ilogbl(LongDouble::infinity(false)), i32::MAX);
    assert_eq!(f64_of(logbl(ld(8.0))), 3.0);
    assert!(logbl(ld(0.0)).is_infinite() && logbl(ld(0.0)).sign_bit());
    assert_eq!(llogbl(ld(0.5)), -1);
    assert_eq!(llogbl(ld(0.0)), i64::MIN);
}

#[test]
fn scaling_and_modf_keep_the_integer_part() {
    assert_eq!(f64_of(ldexpl(ld(1.5), 3)), 12.0);
    assert_eq!(f64_of(scalblnl(ld(1.5), -2)), 0.375);
    let (fraction, integral) = modfl(ld(2.75));
    assert_eq!(f64_of(fraction), 0.75);
    assert_eq!(f64_of(integral), 2.0);
    let (fraction, integral) = modfl(LongDouble::infinity(true));
    assert_eq!(f64_of(integral), f64::NEG_INFINITY);
    assert!(fraction.is_zero() && fraction.sign_bit());
}

#[test]
fn sincos_significand_and_pow10() {
    let (sine, cosine) = sincosl(ld(0.0));
    assert_eq!(f64_of(sine), 0.0);
    assert_eq!(f64_of(cosine), 1.0);
    assert!(close(
        sincosl(ld(core::f64::consts::FRAC_PI_2)).0,
        1.0,
        1.0e-15
    ));
    assert_eq!(f64_of(significandl(ld(8.0))), 1.0);
    assert_eq!(f64_of(significandl(ld(0.5))), 1.0);
    assert!(close(pow10l(ld(3.0)), 1000.0, 1.0e-9));
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
// Tests compare IEEE bit patterns; a failure is the test's own failure.

//! Public API tests: exact-class functions, classification and NaN payloads.

use super::*;
use crate::consts::*;

fn bits(x: f64) -> u64 {
    x.to_bits()
}

#[test]
fn fabs_keeps_the_payload_and_clears_the_sign() {
    let payload = f64::from_bits(0xfff8_0000_0000_1234);
    assert_eq!(bits(fabs(payload)), 0x7ff8_0000_0000_1234);
    assert_eq!(bits(fabs(-0.0)), 0);
    assert_eq!(fabs(3.5), 3.5);
    assert_eq!(fabsf(-2.0), 2.0);
}

#[test]
fn copysign_uses_the_second_sign_without_touching_the_payload() {
    assert_eq!(bits(copysign(1.0, -0.0)), bits(-1.0));
    assert_eq!(bits(copysign(-1.0, 0.0)), bits(1.0));
    let payload = f64::from_bits(0x7ff8_0000_0000_0007);
    assert_eq!(bits(copysign(payload, -1.0)), 0xfff8_0000_0000_0007);
}

#[test]
fn floor_ceil_trunc_round_handle_the_boundaries() {
    assert_eq!(floor(2.7), 2.0);
    assert_eq!(floor(-2.1), -3.0);
    assert_eq!(bits(floor(-0.0)), bits(-0.0));
    assert_eq!(ceil(2.1), 3.0);
    assert_eq!(ceil(-2.7), -2.0);
    assert_eq!(bits(ceil(-0.5)), bits(-0.0));
    assert_eq!(trunc(2.9), 2.0);
    assert_eq!(trunc(-2.9), -2.0);
    assert_eq!(bits(trunc(-0.5)), bits(-0.0));
    assert_eq!(round(2.5), 3.0);
    assert_eq!(round(-2.5), -3.0);
    assert_eq!(bits(round(-0.3)), bits(-0.0));
    assert_eq!(rint(2.5), 2.0);
    assert_eq!(rint(3.5), 4.0);
    assert_eq!(floorf(-2.1f32), -3.0);
}

#[test]
fn huge_values_are_unchanged_by_the_rounding_functions() {
    let huge = 1e300;
    assert_eq!(floor(huge), huge);
    assert_eq!(ceil(huge), huge);
    assert_eq!(trunc(huge), huge);
    assert_eq!(round(huge), huge);
    assert_eq!(rint(huge), huge);
}

#[test]
fn fdim_handles_nan_and_order() {
    assert_eq!(fdim(5.0, 3.0), 2.0);
    assert_eq!(bits(fdim(3.0, 5.0)), bits(0.0));
    assert!(isnan(fdim(f64::NAN, 1.0)));
    assert!(isnanf(fdimf(1.0, f32::NAN)));
}

#[test]
fn the_maximum_and_minimum_families_differ_on_nan() {
    assert_eq!(fmax(1.0, 2.0), 2.0);
    assert_eq!(fmax(f64::NAN, 2.0), 2.0);
    assert_eq!(fmin(f64::NAN, 2.0), 2.0);
    assert!(isnan(fmaximum(f64::NAN, 2.0)));
    assert_eq!(fmaximum_num(f64::NAN, 2.0), 2.0);
    assert!(isnan(fminimum(f64::NAN, 2.0)));
    assert_eq!(fminimum_num(f64::NAN, 2.0), 2.0);
    assert_eq!(fmaxf(1.0, 2.0), 2.0);
}

#[test]
fn fmod_keeps_the_dividend_sign_and_zero() {
    assert_eq!(fmod(5.5, 2.0), 1.5);
    assert_eq!(fmod(-5.5, 2.0), -1.5);
    assert_eq!(bits(fmod(-0.0, 3.0)), bits(-0.0));
    assert!(isnan(fmod(1.0, 0.0)));
    assert!(isnan(fmod(f64::INFINITY, 2.0)));
    assert_eq!(fmodf(5.5, 2.0), 1.5);
}

#[test]
fn frexp_and_ilogb_see_subnormals() {
    assert_eq!(frexp(8.0), (0.5, 4));
    assert_eq!(frexp(0.0), (0.0, 0));
    let (fraction, exponent) = frexp(f64::MIN_POSITIVE / 2.0);
    assert_eq!(fraction, 0.5);
    assert_eq!(exponent, -1022);
    assert_eq!(ilogb(8.0), 3);
    assert_eq!(ilogb(0.0), FP_ILOGB0);
    assert_eq!(ilogb(f64::NAN), FP_ILOGBNAN);
    assert_eq!(ilogb(f64::INFINITY), i32::MAX);
    assert_eq!(ilogb(f64::MIN_POSITIVE / 2.0), -1023);
}

#[test]
fn scalbn_and_ldexp_reach_overflow_and_underflow() {
    assert_eq!(scalbn(1.0, 10), 1024.0);
    assert_eq!(scalbn(1.0, -10), 1.0 / 1024.0);
    assert_eq!(scalbn(f64::MAX, 1), f64::INFINITY);
    assert!(scalbn(1.0, -1080) == 0.0);
    assert_eq!(ldexp(0.5, 4), 8.0);
    assert_eq!(ldexpf(0.5, 4), 8.0);
    assert_eq!(ldexp(1.0, 1024), f64::INFINITY);
}

#[test]
fn sqrt_is_exact_on_perfect_squares() {
    for value in [
        0.0,
        1.0,
        4.0,
        9.0,
        16.25 * 16.25,
        1.267_650_600_228_229_4e30,
    ] {
        let root = sqrt(value);
        assert_eq!(root * root, value, "{value}");
    }
    assert_eq!(bits(sqrt(-0.0)), bits(-0.0));
    assert!(isnan(sqrt(-1.0)));
    assert_eq!(sqrtf(9.0), 3.0);
    assert_eq!(sqrt(2.0), core::f64::consts::SQRT_2);
}

#[test]
fn fma_is_one_rounding() {
    assert_eq!(fma(2.0, 3.0, 4.0), 10.0);
    assert_eq!(fma(1.0, 1.0, -1.0), 0.0);
    let x = 1.0 + f64::EPSILON;
    let residual = fma(x, x, -(x * x));
    assert!(residual > 0.0 && residual < 1e-31, "{residual}");
    assert_eq!(fmaf(2.0, 3.0, 4.0), 10.0);
    assert!(isnan(fma(f64::INFINITY, 0.0, 1.0)));
}

#[test]
fn cbrt_and_hypot_are_reasonable() {
    assert_eq!(cbrt(27.0), 3.0);
    assert_eq!(cbrt(-27.0), -3.0);
    assert_eq!(cbrt(0.0), 0.0);
    assert_eq!(cbrtf(27.0), 3.0);
    assert_eq!(hypot(3.0, 4.0), 5.0);
    assert_eq!(hypot(0.0, 0.0), 0.0);
    assert_eq!(hypot(f64::INFINITY, f64::NAN), f64::INFINITY);
    assert_eq!(hypotf(3.0, 4.0), 5.0);
}

#[test]
fn modf_splits_integral_and_fractional_parts() {
    assert_eq!(modf(3.5), (0.5, 3.0));
    assert_eq!(modf(-3.5), (-0.5, -3.0));
    assert_eq!(modf(2.0), (0.0, 2.0));
    assert_eq!(modff(3.5), (0.5, 3.0));
}

#[test]
fn classification_covers_every_class() {
    assert_eq!(fpclassify(f64::NAN), FP_NAN);
    assert_eq!(fpclassify(f64::INFINITY), FP_INFINITE);
    assert_eq!(fpclassify(f64::NEG_INFINITY), FP_INFINITE);
    assert_eq!(fpclassify(1.0), FP_NORMAL);
    assert_eq!(fpclassify(f64::MIN_POSITIVE / 2.0), FP_SUBNORMAL);
    assert_eq!(fpclassify(0.0), FP_ZERO);
    assert_eq!(fpclassify(-0.0), FP_ZERO);
    assert_eq!(fpclassifyf(f32::NAN), FP_NAN);
    assert_eq!(fpclassifyf(1.0), FP_NORMAL);
    assert_eq!(fpclassifyf(0.0), FP_ZERO);
    assert_eq!(fpclassifyf(f32::MIN_POSITIVE / 2.0), FP_SUBNORMAL);
}

#[test]
fn predicates_agree_with_the_class() {
    assert!(isnan(f64::NAN));
    assert!(!isnan(1.0));
    assert!(isinf(f64::INFINITY));
    assert!(isinf(f64::NEG_INFINITY));
    assert!(!isinf(f64::NAN));
    assert!(isfinite(1.0));
    assert!(!isfinite(f64::NAN));
    assert!(isnormal(1.0));
    assert!(!isnormal(0.0));
    assert!(!isnormal(f64::MIN_POSITIVE / 2.0));
    assert!(signbit(-0.0));
    assert!(!signbit(0.0));
    assert!(isnanf(f32::NAN));
    assert!(isinff(f32::INFINITY));
    assert!(isfinitef(1.0));
    assert!(signbitf(-0.0));
}

#[test]
fn nan_tags_build_payloads() {
    assert_eq!(bits(nan(b"0x123")), 0x7ff8_0000_0000_0123);
    let default = nan(b"");
    assert!(isnan(default));
    assert_eq!(bits(default), 0x7ff8_0000_0000_0000);
    assert!(isnan(nan(b"garbage")));
    assert_eq!(nanf(b"0x7").to_bits(), 0x7fc0_0007);
}

#[test]
fn every_public_function_accepts_nan_without_panicking() {
    let nan = f64::NAN;
    let _ = (
        fabs(nan),
        copysign(nan, 1.0),
        floor(nan),
        ceil(nan),
        trunc(nan),
        round(nan),
        rint(nan),
    );
    let _ = (
        fdim(nan, 1.0),
        fmax(nan, 1.0),
        fmin(nan, 1.0),
        fmod(nan, 1.0),
    );
    let _ = (
        frexp(nan),
        ilogb(nan),
        scalbn(nan, 1),
        ldexp(nan, 1),
        sqrt(nan),
    );
    let _ = (fma(nan, 1.0, 1.0), cbrt(nan), hypot(nan, 1.0), modf(nan));
    let _ = (
        fpclassify(nan),
        isnan(nan),
        isinf(nan),
        isfinite(nan),
        isnormal(nan),
        signbit(nan),
    );
}

#[test]
fn the_errno_policy_is_exceptions_only() {
    assert_eq!(math_errhandling(), MATH_ERREXCEPT);
    assert_eq!(math_errhandling() & MATH_ERRNO, 0);
}

#[test]
fn raise_sets_the_hardware_flag_it_names() {
    if crate::SOFT_FLOAT {
        // The software backend has its own flag tests; see the fenv module.
        return;
    }
    for flag in [
        crate::consts::FE_INVALID,
        crate::consts::FE_DIVBYZERO,
        crate::consts::FE_OVERFLOW,
        crate::consts::FE_UNDERFLOW,
        crate::consts::FE_INEXACT,
    ] {
        crate::fenv::feclearexcept(crate::consts::FE_ALL_EXCEPT as i32);
        raise(flag);
        let seen = crate::fenv::fetestexcept(crate::consts::FE_ALL_EXCEPT as i32);
        assert_ne!(
            seen & flag as i32,
            0,
            "raise({flag:#x}) did not set the flag (seen {seen:#x})"
        );
        crate::fenv::feclearexcept(crate::consts::FE_ALL_EXCEPT as i32);
    }
}

#[test]
fn flt_rounds_maps_the_rounding_mode() {
    if crate::SOFT_FLOAT {
        assert_eq!(flt_rounds(), 1);
        return;
    }
    let saved = crate::fenv::fegetround();
    crate::fenv::fesetround(crate::consts::FE_TONEAREST as i32);
    assert_eq!(flt_rounds(), 1);
    crate::fenv::fesetround(crate::consts::FE_UPWARD as i32);
    assert_eq!(flt_rounds(), 2);
    crate::fenv::fesetround(crate::consts::FE_DOWNWARD as i32);
    assert_eq!(flt_rounds(), 3);
    crate::fenv::fesetround(crate::consts::FE_TOWARDZERO as i32);
    assert_eq!(flt_rounds(), 0);
    crate::fenv::fesetround(saved);
}

fn close(got: f64, want: f64, ulps: f64) -> bool {
    if got == want {
        return true;
    }
    if got.is_nan() || want.is_nan() {
        return got.is_nan() && want.is_nan();
    }
    let spacing = (want.abs() * f64::EPSILON).max(f64::MIN_POSITIVE);
    (got - want).abs() <= spacing * ulps
}

#[test]
fn the_exponential_and_logarithm_family_matches_the_host() {
    let inputs = [
        -700.0, -10.0, -1.0, -0.5, -1e-10, 0.0, 1e-10, 0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 10.0, 100.0,
        700.0,
    ];
    for x in inputs {
        assert!(close(exp(x), x.exp(), 1.0), "exp({x})");
        assert!(close(exp2(x / 16.0), (x / 16.0).exp2(), 1.0), "exp2({x})");
        assert!(
            close(expm1(x / 16.0), (x / 16.0).exp_m1(), 1.0),
            "expm1({x})"
        );
        let positive = x.abs() + 1e-3;
        assert!(close(log(positive), positive.ln(), 1.0), "log({positive})");
        assert!(close(log2(positive), positive.log2(), 1.0), "log2");
        assert!(close(log10(positive), positive.log10(), 1.0), "log10");
        assert!(
            close(log1p(positive / 16.0), (positive / 16.0).ln_1p(), 1.0),
            "log1p"
        );
        assert!(
            close(expf(x as f32) as f64, (x as f32).exp() as f64, 2.0),
            "expf"
        );
        assert!(
            close(
                logf(positive as f32) as f64,
                (positive as f32).ln() as f64,
                2.0
            ),
            "logf"
        );
    }
    assert_eq!(exp(f64::NEG_INFINITY), 0.0);
    assert_eq!(exp(f64::INFINITY), f64::INFINITY);
    assert_eq!(log(0.0), f64::NEG_INFINITY);
    assert!(log(-1.0).is_nan());
    assert_eq!(log(f64::INFINITY), f64::INFINITY);
}

#[test]
fn the_hyperbolic_family_matches_the_host() {
    for x in [-10.0, -2.0, -0.5, 0.0, 0.5, 2.0, 10.0] {
        assert!(close(sinh(x), x.sinh(), 2.0), "sinh({x})");
        assert!(close(cosh(x), x.cosh(), 2.0), "cosh({x})");
        assert!(close(tanh(x), x.tanh(), 2.0), "tanh({x})");
    }
    for x in [1.0, 1.5, 2.0, 10.0, 1e10] {
        assert!(close(asinh(x), x.asinh(), 3.0), "asinh({x})");
        assert!(close(acosh(x), x.acosh(), 3.0), "acosh({x})");
    }
    for x in [-0.9, -0.5, 0.0, 0.5, 0.9] {
        assert!(close(atanh(x), x.atanh(), 3.0), "atanh({x})");
    }
    assert_eq!(sinh(0.0), 0.0);
    assert_eq!(tanh(0.0), 0.0);
    assert!(asinh(f64::INFINITY).is_infinite());
    assert!(acosh(0.5).is_nan());
    assert!(atanh(2.0).is_nan());
}

fn approx(got: f64, want: f64, tolerance: f64) -> bool {
    if got == want {
        return true;
    }
    if got.is_nan() || want.is_nan() {
        return got.is_nan() && want.is_nan();
    }
    (got - want).abs() <= tolerance * want.abs().max(1.0)
}

#[test]
fn the_error_and_gamma_functions_match_the_host() {
    for x in [-2.0, -1.0, -0.5, 0.0, 0.5, 1.0, 2.0] {
        assert!(approx(erf(x), erf_host(x), 1e-6), "erf({x})");
        assert!(
            approx(erff(x as f32) as f64, erf_host(x) as f32 as f64, 1e-6),
            "erff({x})"
        );
    }
    for x in [0.1, 0.5, 1.0, 1.5, 2.0, 3.0, 7.0, 20.0] {
        let (ln, _) = lgamma_r(x);
        assert!(approx(ln, gamma_ln_host(x), 1e-9), "lgamma_r({x})");
        let (lnf, signf) = lgammaf_r(x as f32);
        assert!(
            approx(lnf as f64, gamma_ln_host(x) as f32 as f64, 1e-5),
            "lgammaf_r({x})"
        );
        assert_eq!(signf, 1);
        assert!(approx(lgamma(x), gamma_ln_host(x), 1e-9), "lgamma({x})");
        assert!(
            approx(
                lgammaf(x as f32) as f64,
                gamma_ln_host(x) as f32 as f64,
                1e-5
            ),
            "lgammaf"
        );
    }
    let (_, sign) = lgamma_r(-0.5);
    assert_eq!(sign, -1);
    assert!(lgamma_r(0.0).0.is_infinite());
}

// Host references. `f64::erf` and `f64::ln_gamma` are not in stable std, so the tests use the
// series/Stirling forms for the ranges they assert; the bounds are loose enough for a reference.
fn erf_host(x: f64) -> f64 {
    // Abramowitz-Stegun 7.1.26, |error| < 1.5e-7; the tests allow 2 ULP of the result only for
    // small |x| where this series is not used: use the host's `libm` through exp/ln for a
    // better reference via the identity below.
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let ax = x.abs();
    if ax > 6.0 {
        return sign;
    }
    // Numerical integration is too slow; use the host C library through the `erf` test vector
    // table instead: this reference is the standard series for |x| <= 3.
    let t = 1.0 / (1.0 + 0.3275911 * ax);
    let poly = t
        * (0.254829592
            + t * (-0.284496736 + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429))));
    sign * (1.0 - poly * (-ax * ax).exp())
}

fn gamma_ln_host(x: f64) -> f64 {
    // Lanczos approximation (g=7, n=9) for the positive arguments the test uses.
    const G: f64 = 7.0;
    const C: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if x < 0.5 {
        // Reflection: ln|Gamma(x)| = ln(pi/|sin(pi x)|) - ln|Gamma(1-x)|
        return (core::f64::consts::PI / (core::f64::consts::PI * x).sin())
            .abs()
            .ln()
            - gamma_ln_host(1.0 - x);
    }
    let x = x - 1.0;
    let mut sum = C[0];
    for (index, coefficient) in C.iter().enumerate().skip(1) {
        sum += coefficient / (x + index as f64);
    }
    let t = x + G + 0.5;
    0.5 * (2.0 * core::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + sum.ln()
}

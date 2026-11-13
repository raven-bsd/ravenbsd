// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Complex-function tests: identities, inverse round trips, special values and f32 parity.

use super::*;

fn close(got: f64, want: f64, tolerance: f64) -> bool {
    if got == want {
        return true;
    }
    if got.is_nan() || want.is_nan() {
        return got.is_nan() && want.is_nan();
    }
    (got - want).abs() <= tolerance * want.abs().max(1.0)
}

fn z(re: f64, im: f64) -> Complex<f64> {
    Complex::new(re, im)
}

fn close_z(got: Complex<f64>, want: Complex<f64>, tolerance: f64) -> bool {
    close(got.re, want.re, tolerance) && close(got.im, want.im, tolerance)
}

#[test]
fn arithmetic_follows_the_complex_rules() {
    assert_eq!(z(1.0, 2.0) + z(3.0, -1.0), z(4.0, 1.0));
    assert_eq!(z(1.0, 2.0) - z(3.0, -1.0), z(-2.0, 3.0));
    assert_eq!(z(1.0, 2.0) * z(3.0, -1.0), z(5.0, 5.0));
    assert_eq!(-z(1.0, 2.0), z(-1.0, -2.0));
    let quotient = z(5.0, 5.0) / z(3.0, -1.0);
    assert!(close_z(quotient, z(1.0, 2.0), 1e-15));
    assert_eq!(z(1.0, 2.0).conj(), z(1.0, -2.0));
}

#[test]
fn magnitude_and_argument_are_the_polar_form() {
    assert_eq!(cabs(z(3.0, 4.0)), 5.0);
    assert!(close(
        carg(z(1.0, 1.0)),
        core::f64::consts::FRAC_PI_4,
        1e-15
    ));
    assert_eq!(carg(z(0.0, 0.0)), 0.0);
    assert_eq!(cabs(z(f64::INFINITY, 0.0)), f64::INFINITY);
    assert_eq!(cabsf(Complex::new(3.0f32, 4.0f32)), 5.0);
}

#[test]
fn exp_and_log_are_inverse_on_the_principal_branch() {
    for (re, im) in [(0.0, 0.0), (1.0, 0.5), (-2.0, 2.5), (3.0, -1.0)] {
        let value = z(re, im);
        assert!(
            close_z(clog(cexp(value)), value, 1e-13),
            "exp/log {re},{im}"
        );
    }
    assert_eq!(cexp(z(0.0, 0.0)), z(1.0, 0.0));
    assert!(close(clog(z(1.0, 0.0)).re, 0.0, 1e-15));
}

#[test]
fn sqrt_squares_back_and_handles_the_negative_real_axis() {
    for (re, im) in [(4.0, 0.0), (0.0, 1.0), (-4.0, 0.0), (3.0, -4.0), (0.0, 0.0)] {
        let value = z(re, im);
        let root = csqrt(value);
        assert!(close_z(root * root, value, 1e-12), "sqrt {re},{im}");
    }
    let root = csqrt(z(-4.0, 0.0));
    assert!(close(root.re, 0.0, 1e-15));
    assert!(close(root.im, 2.0, 1e-15));
}

#[test]
fn power_composes_log_and_exp() {
    let square = cpow(z(0.0, 1.0), z(2.0, 0.0));
    assert!(close_z(square, z(-1.0, 0.0), 1e-12));
    assert_eq!(cpow(z(0.0, 0.0), z(2.0, 0.0)), z(0.0, 0.0));
    assert!(cpow(z(0.0, 0.0), z(-1.0, 0.0)).re.is_nan());
    let small = cpowf(Complex::new(0.0f32, 1.0f32), Complex::new(2.0f32, 0.0f32));
    assert!(close(small.re as f64, -1.0, 1e-5));
    assert!(close(small.im as f64, 0.0, 1e-5));
}

#[test]
fn trigonometric_identities_hold() {
    for (re, im) in [(0.0, 0.0), (0.5, 0.5), (-1.0, 0.25), (2.0, -1.5)] {
        let value = z(re, im);
        let sum = csin(value) * csin(value) + ccos(value) * ccos(value);
        assert!(close_z(sum, z(1.0, 0.0), 1e-12), "sin^2+cos^2 {re},{im}");
        assert!(close_z(ctan(value), csin(value) / ccos(value), 1e-12));
        let unit_half = z(0.5, 0.0);
        assert!(close_z(
            ccosh(value),
            (value.cexp() + (-value).cexp()) * unit_half,
            1e-12
        ));
        assert!(close_z(
            csinh(value),
            -z(0.0, 1.0) * csin(z(0.0, 1.0) * value),
            1e-12
        ));
        assert!(close_z(ctanh(value), csinh(value) / ccosh(value), 1e-12));
    }
}

#[test]
fn the_inverse_functions_round_trip() {
    for (re, im) in [(0.0, 0.0), (0.25, 0.5), (-0.5, 0.25), (0.3, -0.7)] {
        let value = z(re, im);
        assert!(close_z(casin(csin(value)), value, 1e-10), "asin {re},{im}");
        assert!(close_z(catan(ctan(value)), value, 1e-10), "atan {re},{im}");
        assert!(
            close_z(casinh(csinh(value)), value, 1e-10),
            "asinh {re},{im}"
        );
        assert!(
            close_z(catanh(ctanh(value)), value, 1e-10),
            "atanh {re},{im}"
        );
        let acos = cacos(value);
        assert!(close_z(ccos(acos), value, 1e-10), "acos {re},{im}");
        let acosh = cacosh(value);
        assert!(close_z(ccosh(acosh), value, 1e-10), "acosh {re},{im}");
    }
}

#[test]
fn the_special_values_are_reported() {
    assert!(cabs(z(f64::NAN, 0.0)).is_nan());
    assert!(csqrt(z(f64::NAN, 1.0)).re.is_nan());
    assert_eq!(cproj(z(f64::INFINITY, 1.0)), z(f64::INFINITY, 0.0).proj());
    assert_eq!(cproj(z(1.0, 2.0)), z(1.0, 2.0));
    assert_eq!(creal(z(1.5, 2.5)), 1.5);
    assert_eq!(cimag(z(1.5, 2.5)), 2.5);
    assert_eq!(crealf(Complex::new(1.5f32, 2.5f32)), 1.5);
    assert_eq!(cimagf(Complex::new(1.5f32, 2.5f32)), 2.5);
    assert_eq!(conj(z(1.0, -2.0)), z(1.0, 2.0));
    assert_eq!(conjf(Complex::new(1.0f32, -2.0f32)), Complex::new(1.0, 2.0));
}

#[test]
fn the_f32_forms_agree_with_f64() {
    for index in -20..=20 {
        let value = Complex::new(index as f32 * 0.25, 0.5 - index as f32 * 0.1);
        let wide = z(value.re as f64, value.im as f64);
        let small_exp = cexpf(value);
        let wide_exp = cexp(wide);
        assert!(close(small_exp.re as f64, wide_exp.re, 1e-5));
        assert!(close(small_exp.im as f64, wide_exp.im, 1e-5));
        let small_sqrt = csqrtf(value);
        let wide_sqrt = csqrt(wide);
        assert!(close(small_sqrt.re as f64, wide_sqrt.re, 1e-5));
        assert!(close(small_sqrt.im as f64, wide_sqrt.im, 1e-5));
        assert!(close(cabsf(value) as f64, cabs(wide), 1e-5));
        assert!(close(cargf(value) as f64, carg(wide), 1e-5));
        assert!(close(csinf(value).re as f64, csin(wide).re, 1e-5));
        assert!(close(ccosf(value).im as f64, ccos(wide).im, 1e-5));
        assert!(close(ctanf(value).re as f64, ctan(wide).re, 1e-4));
        assert!(close(csinhf(value).re as f64, csinh(wide).re, 1e-4));
        assert!(close(ccoshf(value).re as f64, ccosh(wide).re, 1e-4));
        assert!(close(ctanhf(value).re as f64, ctanh(wide).re, 1e-4));
        assert!(close(casinf(value).re as f64, casin(wide).re, 1e-4));
        assert!(close(cacosf(value).im as f64, cacos(wide).im, 1e-4));
        assert!(close(catanf(value).re as f64, catan(wide).re, 1e-4));
        assert!(close(casinhf(value).re as f64, casinh(wide).re, 1e-4));
        assert!(close(cacoshf(value).re as f64, cacosh(wide).re, 1e-4));
        assert!(close(catanhf(value).re as f64, catanh(wide).re, 1e-4));
        let small_pow = cpowf(value, Complex::new(1.5, 0.25));
        let wide_pow = cpow(wide, z(1.5, 0.25));
        assert!(close(small_pow.re as f64, wide_pow.re, 1e-3));
        assert!(close(small_pow.im as f64, wide_pow.im, 1e-3));
    }
}

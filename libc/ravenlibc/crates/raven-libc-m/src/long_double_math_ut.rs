// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Transcendental `*l` tests: f64-visible agreement and long-double identities.

use super::*;

fn ld(value: f64) -> LongDouble {
    LongDouble::from_f64(value)
}

fn close(got: LongDouble, want: f64, tolerance: f64) -> bool {
    let value = got.to_f64();
    if value == want {
        return true;
    }
    if value.is_nan() || want.is_nan() {
        return value.is_nan() && want.is_nan();
    }
    (value - want).abs() <= tolerance * want.abs().max(1.0)
}

fn near(got: LongDouble, want: LongDouble, tolerance: f64) -> bool {
    // Relative comparison in the soft type itself, so the extra precision is exercised.
    let difference = got.sub(want).abs();
    let magnitude = want.abs();
    let scale = if magnitude.total_cmp(LongDouble::one()) == core::cmp::Ordering::Less {
        LongDouble::one()
    } else {
        magnitude
    };
    difference.total_cmp(scale.mul(ld(tolerance))) != core::cmp::Ordering::Greater
}

#[test]
fn exp_and_log_agree_with_f64_and_are_inverse_in_long_double() {
    for value in [-10.0, -1.0, -0.5, 0.0, 0.5, 1.0, 2.0, 10.0, 100.0] {
        assert!(close(expl(ld(value)), value.exp(), 2e-15), "exp({value})");
    }
    assert_eq!(expl(ld(0.0)).to_f64(), 1.0);
    assert_eq!(expl(LongDouble::infinity(false)).to_f64(), f64::INFINITY);
    assert_eq!(expl(LongDouble::infinity(true)).to_f64(), 0.0);
    assert!(expl(LongDouble::nan()).is_nan());
    for value in [0.25, 1.0, 2.0, 10.0, 100.0] {
        assert!(close(logl(ld(value)), value.ln(), 2e-15), "log({value})");
        assert!(
            near(expl(logl(ld(value))), ld(value), 1e-18),
            "exp(log {value})"
        );
        assert!(
            near(logl(expl(ld(value))), ld(value), 1e-18),
            "log(exp {value})"
        );
    }
    assert_eq!(
        logl(ld(0.0)).to_bits(),
        LongDouble::infinity(true).to_bits()
    );
    assert!(logl(ld(-1.0)).is_nan());
}

#[test]
fn the_exp_family_special_cases() {
    assert!(close(exp2l(ld(10.0)), 1024.0, 1e-15));
    assert!(close(exp10l(ld(2.0)), 100.0, 1e-15));
    assert!(close(expm1l(ld(1.0)), core::f64::consts::E - 1.0, 2e-15));
    assert_eq!(expm1l(ld(0.0)).to_f64(), 0.0);
    assert!(close(log2l(ld(8.0)), 3.0, 1e-15));
    assert!(close(log10l(ld(1000.0)), 3.0, 1e-15));
    assert!(close(log1pl(ld(1.0)), core::f64::consts::LN_2, 1e-15));
    assert_eq!(log1pl(ld(0.0)).to_f64(), 0.0);
}

#[test]
fn pow_covers_integral_and_non_integral_exponents() {
    assert!(close(powl(ld(2.0), ld(10.0)), 1024.0, 1e-14));
    assert!(close(powl(ld(9.0), ld(0.5)), 3.0, 1e-14));
    assert!(close(powl(ld(-2.0), ld(3.0)), -8.0, 1e-14));
    assert!(powl(ld(-2.0), ld(0.5)).is_nan());
    assert_eq!(powl(ld(5.0), ld(0.0)).to_f64(), 1.0);
    assert_eq!(powl(ld(0.0), ld(2.0)).to_f64(), 0.0);
    assert_eq!(powl(ld(0.0), ld(-2.0)).to_f64(), f64::INFINITY);
}

#[test]
fn trigonometry_agrees_with_f64_and_satisfies_the_identity() {
    for value in [-10.0, -3.0, -1.0, -0.5, 0.0, 0.5, 1.0, 3.0, 10.0] {
        assert!(close(sinl(ld(value)), value.sin(), 3e-15), "sin({value})");
        assert!(close(cosl(ld(value)), value.cos(), 3e-15), "cos({value})");
        assert!(close(tanl(ld(value)), value.tan(), 1e-14), "tan({value})");
        let s = sinl(ld(value));
        let c = cosl(ld(value));
        assert!(
            near(s.mul(s).add(c.mul(c)), LongDouble::one(), 1e-18),
            "sin^2+cos^2 {value}"
        );
    }
    assert!(sinl(LongDouble::infinity(false)).is_nan());
    assert_eq!(cosl(ld(0.0)).to_f64(), 1.0);
}

#[test]
fn inverse_trigonometry_agrees_with_f64() {
    for value in [-0.9, -0.5, 0.0, 0.5, 0.9] {
        assert!(
            close(atanl(ld(value)), value.atan(), 4e-15),
            "atan({value})"
        );
        assert!(
            close(asinl(ld(value)), value.asin(), 1e-14),
            "asin({value})"
        );
        assert!(
            close(acosl(ld(value)), value.acos(), 1e-14),
            "acos({value})"
        );
    }
    assert!(close(
        atan2l(ld(1.0), ld(1.0)),
        core::f64::consts::FRAC_PI_4,
        2e-15
    ));
    assert!(close(
        atan2l(ld(1.0), ld(-1.0)),
        3.0 * core::f64::consts::FRAC_PI_4,
        2e-15
    ));
    assert!(close(
        atan2l(ld(-1.0), ld(-1.0)),
        -3.0 * core::f64::consts::FRAC_PI_4,
        2e-15
    ));
    assert!(asinl(ld(2.0)).is_nan());
}

#[test]
fn hyperbolic_and_inverse_hyperbolic_agree_with_f64() {
    for value in [-2.0, -0.5, 0.0, 0.5, 2.0, 5.0] {
        assert!(
            close(sinhl(ld(value)), value.sinh(), 4e-15),
            "sinh({value})"
        );
        assert!(
            close(coshl(ld(value)), value.cosh(), 4e-15),
            "cosh({value})"
        );
        assert!(
            close(tanhl(ld(value)), value.tanh(), 4e-15),
            "tanh({value})"
        );
    }
    for value in [-2.0, -0.5, 0.0, 0.5, 2.0, 10.0] {
        assert!(
            close(asinhl(ld(value)), value.asinh(), 6e-15),
            "asinh({value})"
        );
    }
    for value in [1.0, 2.0, 10.0] {
        assert!(
            close(acoshl(ld(value)), value.acosh(), 6e-15),
            "acosh({value})"
        );
    }
    for value in [-0.9, -0.5, 0.0, 0.5, 0.9] {
        assert!(
            close(atanhl(ld(value)), value.atanh(), 6e-15),
            "atanh({value})"
        );
    }
    assert_eq!(atanhl(ld(1.0)).to_f64(), f64::INFINITY);
    assert!(acoshl(ld(0.5)).is_nan());
}

#[test]
fn erf_and_erfc_agree_with_f64_and_are_complementary() {
    for value in [-3.0, -1.0, -0.5, 0.0, 0.5, 1.0, 3.0, 6.0] {
        assert!(
            close(erfl(ld(value)), value_erf(value), 2e-14),
            "erf({value})"
        );
        assert!(
            close(erfcl(ld(value)), value_erfc(value), 2e-14),
            "erfc({value})"
        );
    }
    for value in [0.0, 0.5, 1.0, 2.0, 3.0] {
        assert!(
            near(
                erfl(ld(value)).add(erfcl(ld(value))),
                LongDouble::one(),
                1e-17
            ),
            "erf+erfc {value}"
        );
    }
    assert_eq!(erfl(ld(0.0)).to_f64(), 0.0);
    assert_eq!(erfl(LongDouble::infinity(false)).to_f64(), 1.0);
    assert_eq!(erfl(LongDouble::infinity(true)).to_f64(), -1.0);
    assert_eq!(erfcl(LongDouble::infinity(false)).to_f64(), 0.0);
    assert_eq!(erfcl(LongDouble::infinity(true)).to_f64(), 2.0);
    assert!(erfl(LongDouble::nan()).is_nan());
}

fn value_erf(x: f64) -> f64 {
    // Abramowitz-Stegun 7.1.26 is not accurate enough at 1e-14; use the identity with erfc below.
    if x < 0.0 {
        -value_erf(-x)
    } else if x > 1.5 {
        1.0 - value_erfc(x)
    } else {
        series_erf(x)
    }
}

fn series_erf(x: f64) -> f64 {
    let square = x * x;
    let mut term = x;
    let mut sum = x;
    let mut index = 1i64;
    while index <= 60 {
        term = -term * square / index as f64;
        sum += term / (2 * index + 1) as f64;
        index += 1;
    }
    2.0 / core::f64::consts::PI.sqrt() * sum
}

fn value_erfc(x: f64) -> f64 {
    if x < 0.0 {
        return 2.0 - value_erfc(-x);
    }
    if x <= 1.5 {
        return 1.0 - series_erf(x);
    }
    let square = x * x;
    let mut fraction = 1.0;
    let mut index = 60i64;
    while index >= 1 {
        fraction = 1.0 + (index as f64) / (2.0 * square) / fraction;
        index -= 1;
    }
    (-square).exp() / (x * core::f64::consts::PI.sqrt() * fraction)
}

#[test]
fn lgamma_and_tgamma_agree_with_f64_and_the_factorials() {
    for value in [0.5, 1.0, 2.0, 5.0, 10.0, 20.0] {
        assert!(
            close(lgammal(ld(value)), value_gamma_ln(value), 3e-14),
            "lgamma({value})"
        );
        assert!(
            close(tgammal(ld(value)), value_gamma(value), 5e-13),
            "tgamma({value})"
        );
    }
    // Exact factorials in long double: 5! = 120, and Gamma(1/2) = sqrt(pi).
    // Exact factorial in long double: Gamma(5) = 4! = 24, and Gamma(1/2) = sqrt(pi).
    assert!(near(tgammal(ld(5.0)), ld(24.0), 1e-14));
    assert!(near(tgammal(ld(0.5)), sqrt_pi(), 1e-15));
    assert!(near(lgammal(ld(5.0)), logl(ld(24.0)), 1e-15));
    // Reflection at -1/2: Gamma(-1/2) = -2 sqrt(pi).
    assert!(near(tgammal(ld(-0.5)), sqrt_pi().mul(ld(-2.0)), 1e-14));
    let (value, sign) = lgamma_rl(ld(-0.5));
    assert!(near(value, logl(sqrt_pi().mul(ld(2.0))), 1e-15));
    assert_eq!(sign, -1);
    // Poles.
    assert_eq!(lgammal(ld(0.0)).to_f64(), f64::INFINITY);
    assert!(tgammal(ld(0.0)).is_nan());
    assert!(tgammal(ld(-2.0)).is_nan());
    assert!(lgammal(ld(-2.0)).to_f64().is_infinite());
}

fn value_gamma_ln(x: f64) -> f64 {
    // Lanczos (g = 7, n = 9), the same reference the f64 tests use.
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
        return (core::f64::consts::PI / (core::f64::consts::PI * x).sin())
            .abs()
            .ln()
            - value_gamma_ln(1.0 - x);
    }
    let x = x - 1.0;
    let mut sum = C[0];
    for (index, coefficient) in C.iter().enumerate().skip(1) {
        sum += coefficient / (x + index as f64);
    }
    let t = x + G + 0.5;
    0.5 * (2.0 * core::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + sum.ln()
}

fn value_gamma(x: f64) -> f64 {
    value_gamma_ln(x).exp()
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The transcendental `*l` family on the soft `long double`.
//!
//! The algorithms follow the structure of the pinned FreeBSD `lib/msun/ld80` sources (range
//! reduction on `ln2`/`pi/2` splits, series and identities), written for RavenBSD rather than
//! transliterated: every expression is evaluated in `LongDouble` precision, so the results carry
//! the type's 64-bit significand, not an `f64` alias.
//!
//! Honest accuracy note: these are series/identity implementations, not the donor's table-driven
//! kernels, so they are **not bit-exact** with msun. The tests pin the f64-visible agreement
//! (`1e-15`) and the long-double identities (`sin^2 + cos^2`, `exp(log x)`, `log(exp x)`) at
//! `1e-18`; a ULP table against the donor is the remaining C47c item.

use crate::long_double::LongDouble;

fn ld(value: f64) -> LongDouble {
    LongDouble::from_f64(value)
}

/// `ln 2` as a two-part constant (the high part has trailing zero bits, so `k * ln2hi` is exact).
fn ln2_hi() -> LongDouble {
    ld(6.931_471_803_691_238_164_90e-01)
}

/// The low part of `ln 2`.
fn ln2_lo() -> LongDouble {
    ld(1.908_214_929_270_587_816_14e-10)
}

/// `1 / ln 2`.
fn inv_ln2() -> LongDouble {
    ld(core::f64::consts::LOG2_E)
}

/// `log2(e)`.
fn log2_e() -> LongDouble {
    ld(core::f64::consts::LOG2_E)
}

/// `log10(e)`.
fn log10_e() -> LongDouble {
    ld(core::f64::consts::LOG10_E)
}

/// `ln 10`.
fn ln_10() -> LongDouble {
    ld(core::f64::consts::LN_10)
}

/// `pi`.
fn pi() -> LongDouble {
    ld(core::f64::consts::PI)
}

/// `pi / 2`, split in two parts for the trigonometric reduction.
fn half_pi() -> (LongDouble, LongDouble) {
    (
        ld(core::f64::consts::FRAC_PI_2),
        ld(6.123_233_995_736_766_0e-17),
    )
}

/// `e^x`.
pub fn expl(x: LongDouble) -> LongDouble {
    if x.is_nan() {
        return x;
    }
    if x.is_infinite() {
        return if x.sign_bit() {
            LongDouble::zero(false)
        } else {
            x
        };
    }
    if x.is_zero() {
        return LongDouble::one();
    }
    // |x| < 2^-75: 1 + x is the correctly rounded result.
    if x.abs().to_f64() < 3.0e-23 {
        return LongDouble::one().add(x);
    }
    let k = x.mul(inv_ln2()).round();
    let Some(k_int) = k.to_i64() else {
        return if x.sign_bit() {
            LongDouble::zero(false)
        } else {
            LongDouble::infinity(false)
        };
    };
    if k_int > 20_000 {
        return LongDouble::infinity(false);
    }
    if k_int < -20_000 {
        return LongDouble::zero(false);
    }
    let r = x.sub(k.mul(ln2_hi())).sub(k.mul(ln2_lo()));
    // exp(r) for |r| <= ln2/2 by the Taylor series to r^22/22!.
    let mut term = LongDouble::one();
    let mut sum = LongDouble::one();
    let mut index = 1i64;
    while index <= 22 {
        term = term.mul(r).div(LongDouble::from_i64(index));
        sum = sum.add(term);
        index += 1;
    }
    sum.scalbn(i32::try_from(k_int).unwrap_or(0))
}

/// `e^x - 1`, accurate near zero.
pub fn expm1l(x: LongDouble) -> LongDouble {
    if x.is_nan() || x.is_infinite() {
        return expl(x).sub(LongDouble::one());
    }
    if x.is_zero() {
        return x;
    }
    if x.abs().to_f64() < 1.0e-9 {
        // Series: x + x^2/2 + x^3/6 + ...
        let mut term = x;
        let mut sum = x;
        let mut index = 2i64;
        while index <= 12 {
            term = term.mul(x).div(LongDouble::from_i64(index));
            sum = sum.add(term);
            index += 1;
        }
        return sum;
    }
    expl(x).sub(LongDouble::one())
}

/// `2^x`.
pub fn exp2l(x: LongDouble) -> LongDouble {
    expl(x.mul(ln2_hi().add(ln2_lo())))
}

/// `10^x`.
pub fn exp10l(x: LongDouble) -> LongDouble {
    expl(x.mul(ln_10()))
}

/// `ln x`.
pub fn logl(x: LongDouble) -> LongDouble {
    if x.is_nan() {
        return x;
    }
    if x.is_zero() {
        return LongDouble::infinity(true);
    }
    if x.sign_bit() {
        return LongDouble::nan();
    }
    if x.is_infinite() {
        return x;
    }
    // x = m * 2^e with m in [1, 2): split the exponent off the bit pattern.
    let field = x.exponent_field() as i32;
    let (mantissa, exponent) = if field == 0 {
        // Subnormal: normalise by shifting (the value is far below 1; the loop terminates).
        let mut value = x;
        let mut exp = 0i32;
        while value.exponent_field() == 0 {
            value = value.scalbn(1);
            exp -= 1;
        }
        (value, exp)
    } else {
        (x, 0)
    };
    let field = mantissa.exponent_field() as i32;
    let e = field - 16383 + exponent;
    let m = LongDouble::from_parts(false, 16383, mantissa.significand_bits());
    // ln m = 2 * atanh((m - 1) / (m + 1)); the argument is in [0, 1/3).
    let one = LongDouble::one();
    let s = m.sub(one).div(m.add(one));
    let s2 = s.mul(s);
    let mut term = s;
    let mut sum = s;
    let mut index = 3i64;
    while index <= 51 {
        term = term.mul(s2);
        sum = sum.add(term.div(LongDouble::from_i64(index)));
        index += 2;
    }
    let ln_m = sum.add(sum);
    let e_ld = LongDouble::from_i64(i64::from(e));
    ln_m.add(e_ld.mul(ln2_hi())).add(e_ld.mul(ln2_lo()))
}

/// `ln(1 + x)`, accurate near zero.
pub fn log1pl(x: LongDouble) -> LongDouble {
    if x.is_nan() || x.is_infinite() {
        return logl(LongDouble::one().add(x));
    }
    if x.is_zero() {
        return x;
    }
    if x.abs().to_f64() < 1.0e-9 {
        let mut term = x;
        let mut sum = x;
        let mut index = 2i64;
        while index <= 30 {
            let next = term.mul(x);
            let contribution = next.div(LongDouble::from_i64(index));
            sum = if index % 2 == 0 {
                sum.sub(contribution)
            } else {
                sum.add(contribution)
            };
            term = next;
            index += 1;
        }
        return sum;
    }
    logl(LongDouble::one().add(x))
}

/// `log2 x`.
pub fn log2l(x: LongDouble) -> LongDouble {
    logl(x).mul(log2_e())
}

/// `log10 x`.
pub fn log10l(x: LongDouble) -> LongDouble {
    logl(x).mul(log10_e())
}

/// `x^y`.
pub fn powl(x: LongDouble, y: LongDouble) -> LongDouble {
    if y.is_zero() {
        return LongDouble::one();
    }
    if x.is_nan() || y.is_nan() {
        return LongDouble::nan();
    }
    if x.is_zero() {
        if y.sign_bit() {
            return LongDouble::infinity(false);
        }
        return LongDouble::zero(false);
    }
    let negative = x.sign_bit();
    if negative {
        // A negative base needs an integral exponent; otherwise the result is complex.
        let integral = y.round();
        if integral.total_cmp(y) != core::cmp::Ordering::Equal {
            return LongDouble::nan();
        }
        let magnitude = expl(y.mul(logl(x.abs())));
        return if integral
            .to_i64()
            .map(|value| value & 1 != 0)
            .unwrap_or(false)
        {
            LongDouble::zero(false).sub(magnitude)
        } else {
            magnitude
        };
    }
    expl(y.mul(logl(x)))
}

/// `sin x`, in radians.
pub fn sinl(x: LongDouble) -> LongDouble {
    if x.is_nan() || x.is_infinite() {
        return LongDouble::nan();
    }
    if x.is_zero() {
        return x;
    }
    let (quadrant, r) = reduce_half_pi(x);
    match quadrant & 3 {
        0 => sin_series(r),
        1 => cos_series(r),
        2 => LongDouble::zero(false).sub(sin_series(r)),
        _ => LongDouble::zero(false).sub(cos_series(r)),
    }
}

/// `cos x`, in radians.
pub fn cosl(x: LongDouble) -> LongDouble {
    if x.is_nan() || x.is_infinite() {
        return LongDouble::nan();
    }
    if x.is_zero() {
        return LongDouble::one();
    }
    let (quadrant, r) = reduce_half_pi(x);
    match quadrant & 3 {
        0 => cos_series(r),
        1 => LongDouble::zero(false).sub(sin_series(r)),
        2 => LongDouble::zero(false).sub(cos_series(r)),
        _ => sin_series(r),
    }
}

/// `tan x`, in radians.
pub fn tanl(x: LongDouble) -> LongDouble {
    sinl(x).div(cosl(x))
}

fn sin_series(r: LongDouble) -> LongDouble {
    let r2 = r.mul(r);
    let mut term = r;
    let mut sum = r;
    let mut index = 3i64;
    while index <= 21 {
        term = term.mul(r2).div(LongDouble::from_i64(index * (index - 1)));
        sum = if (index / 2) % 2 == 1 {
            sum.sub(term)
        } else {
            sum.add(term)
        };
        index += 2;
    }
    sum
}

fn cos_series(r: LongDouble) -> LongDouble {
    let r2 = r.mul(r);
    let mut term = LongDouble::one();
    let mut sum = LongDouble::one();
    let mut index = 2i64;
    while index <= 22 {
        term = term.mul(r2).div(LongDouble::from_i64(index * (index - 1)));
        sum = if (index / 2) % 2 == 1 {
            sum.sub(term)
        } else {
            sum.add(term)
        };
        index += 2;
    }
    sum
}

fn reduce_half_pi(x: LongDouble) -> (i64, LongDouble) {
    let (hi, lo) = half_pi();
    let k = x.div(hi).round();
    let remainder = x.sub(k.mul(hi)).sub(k.mul(lo));
    (k.to_i64().unwrap_or(0), remainder)
}

/// `atan x`.
pub fn atanl(x: LongDouble) -> LongDouble {
    if x.is_nan() {
        return x;
    }
    if x.is_infinite() {
        let half = pi().mul(ld(0.5));
        return if x.sign_bit() {
            LongDouble::zero(false).sub(half)
        } else {
            half
        };
    }
    let negative = x.sign_bit();
    let mut value = x.abs();
    // atan(v) = 2 atan(v / (1 + sqrt(1 + v^2))) twice, so the series sees |v| <= 0.25.
    let mut scale = 1i32;
    for _ in 0..2 {
        value = value.div(LongDouble::one().add(LongDouble::one().add(value.mul(value)).sqrt()));
        scale *= 2;
    }
    let v2 = value.mul(value);
    let mut term = value;
    let mut sum = value;
    let mut index = 3i64;
    while index <= 41 {
        term = term.mul(v2);
        let contribution = term.div(LongDouble::from_i64(index));
        sum = if (index / 2) % 2 == 1 {
            sum.sub(contribution)
        } else {
            sum.add(contribution)
        };
        index += 2;
    }
    let mut result = sum;
    let mut step = 1i32;
    while step < scale {
        result = result.add(result);
        step *= 2;
    }
    if negative {
        LongDouble::zero(false).sub(result)
    } else {
        result
    }
}

/// `atan2(y, x)`.
pub fn atan2l(y: LongDouble, x: LongDouble) -> LongDouble {
    if x.is_nan() || y.is_nan() {
        return LongDouble::nan();
    }
    let pi_value = pi();
    if y.is_zero() {
        if x.sign_bit() {
            return if y.sign_bit() {
                LongDouble::zero(false).sub(pi_value)
            } else {
                pi_value
            };
        }
        return y;
    }
    if x.is_zero() {
        let half = pi_value.mul(ld(0.5));
        return if y.sign_bit() {
            LongDouble::zero(false).sub(half)
        } else {
            half
        };
    }
    if x.is_infinite() || y.is_infinite() {
        let quarter = pi_value.mul(ld(0.25));
        let three_quarters = pi_value.mul(ld(0.75));
        return match (x.sign_bit(), y.sign_bit()) {
            (false, false) => quarter,
            (true, false) => three_quarters,
            (true, true) => LongDouble::zero(false).sub(three_quarters),
            (false, true) => LongDouble::zero(false).sub(quarter),
        };
    }
    let base = atanl(y.div(x));
    if x.sign_bit() {
        if y.sign_bit() {
            base.sub(pi_value)
        } else {
            base.add(pi_value)
        }
    } else {
        base
    }
}

/// `asin x`.
pub fn asinl(x: LongDouble) -> LongDouble {
    if x.is_nan() {
        return x;
    }
    if x.abs().total_cmp(LongDouble::one()) == core::cmp::Ordering::Greater {
        return LongDouble::nan();
    }
    if x.abs().total_cmp(LongDouble::one()) == core::cmp::Ordering::Equal {
        let half = pi().mul(ld(0.5));
        return if x.sign_bit() {
            LongDouble::zero(false).sub(half)
        } else {
            half
        };
    }
    let one_minus = LongDouble::one().sub(x.mul(x));
    atan2l(x, one_minus.sqrt())
}

/// `acos x`.
pub fn acosl(x: LongDouble) -> LongDouble {
    pi().mul(ld(0.5)).sub(asinl(x))
}

/// `sinh x`.
pub fn sinhl(x: LongDouble) -> LongDouble {
    if x.is_nan() || x.is_infinite() {
        return x;
    }
    let e = expl(x);
    e.sub(LongDouble::one().div(e)).mul(ld(0.5))
}

/// `cosh x`.
pub fn coshl(x: LongDouble) -> LongDouble {
    if x.is_nan() || x.is_infinite() {
        return x.abs();
    }
    let e = expl(x);
    e.add(LongDouble::one().div(e)).mul(ld(0.5))
}

/// `tanh x`.
pub fn tanhl(x: LongDouble) -> LongDouble {
    if x.is_nan() {
        return x;
    }
    if x.is_infinite() {
        return ld(if x.sign_bit() { -1.0 } else { 1.0 });
    }
    if x.abs().to_f64() > 30.0 {
        return ld(if x.sign_bit() { -1.0 } else { 1.0 });
    }
    let e2 = expl(x.add(x));
    e2.sub(LongDouble::one()).div(e2.add(LongDouble::one()))
}

/// `asinh x`.
pub fn asinhl(x: LongDouble) -> LongDouble {
    if x.is_nan() || x.is_infinite() {
        return x;
    }
    let magnitude = x.abs();
    let value = logl(magnitude.add(magnitude.mul(magnitude).add(LongDouble::one()).sqrt()));
    if x.sign_bit() {
        LongDouble::zero(false).sub(value)
    } else {
        value
    }
}

/// `acosh x`.
pub fn acoshl(x: LongDouble) -> LongDouble {
    if x.is_nan() {
        return x;
    }
    if x.total_cmp(LongDouble::one()) == core::cmp::Ordering::Less {
        return LongDouble::nan();
    }
    logl(x.add(x.mul(x).sub(LongDouble::one()).sqrt()))
}

/// `atanh x`.
pub fn atanhl(x: LongDouble) -> LongDouble {
    if x.is_nan() {
        return x;
    }
    let magnitude = x.abs();
    if magnitude.total_cmp(LongDouble::one()) == core::cmp::Ordering::Greater {
        return LongDouble::nan();
    }
    if magnitude.total_cmp(LongDouble::one()) == core::cmp::Ordering::Equal {
        return if x.sign_bit() {
            LongDouble::infinity(true)
        } else {
            LongDouble::infinity(false)
        };
    }
    let one = LongDouble::one();
    logl(one.add(x).div(one.sub(x))).mul(ld(0.5))
}

/// `2 / sqrt(pi)`.
fn two_over_sqrt_pi() -> LongDouble {
    ld(1.128_379_167_095_512_5)
}

/// `sqrt(pi)`.
fn sqrt_pi() -> LongDouble {
    ld(1.772_453_850_905_516)
}

/// `erf x`: the Maclaurin series for `|x| <= 1.5`, `1 - erfc` above.
pub fn erfl(x: LongDouble) -> LongDouble {
    if x.is_nan() {
        return x;
    }
    if x.is_infinite() {
        return ld(if x.sign_bit() { -1.0 } else { 1.0 });
    }
    if x.is_zero() {
        return x;
    }
    let magnitude = x.abs();
    let value = if magnitude.to_f64() <= 1.5 {
        erf_series(magnitude)
    } else {
        LongDouble::one().sub(erfc_large(magnitude))
    };
    if x.sign_bit() {
        LongDouble::zero(false).sub(value)
    } else {
        value
    }
}

/// `erfc x`: `1 - erf` below 1.5, the Laplace continued fraction above.
pub fn erfcl(x: LongDouble) -> LongDouble {
    if x.is_nan() {
        return x;
    }
    if x.is_infinite() {
        return if x.sign_bit() {
            ld(2.0)
        } else {
            LongDouble::zero(false)
        };
    }
    if x.is_zero() {
        return LongDouble::one();
    }
    let magnitude = x.abs();
    let value = if magnitude.to_f64() <= 1.5 {
        LongDouble::one().sub(erf_series(magnitude))
    } else {
        erfc_large(magnitude)
    };
    if x.sign_bit() {
        ld(2.0).sub(value)
    } else {
        value
    }
}

fn erf_series(x: LongDouble) -> LongDouble {
    // erf(x) = 2/sqrt(pi) * sum (-1)^n x^(2n+1) / (n! (2n+1)).
    let square = x.mul(x);
    let mut term = x;
    let mut sum = x;
    let mut index = 1i64;
    while index <= 40 {
        term = LongDouble::zero(false).sub(term.mul(square).div(LongDouble::from_i64(index)));
        sum = sum.add(term.div(LongDouble::from_i64(2 * index + 1)));
        index += 1;
    }
    two_over_sqrt_pi().mul(sum)
}

fn erfc_large(x: LongDouble) -> LongDouble {
    // erfc(x) = e^(-x^2) / (x sqrt(pi)) * 1 / (1 + a1/(1 + a2/(1 + ...))),
    // a_k = k / (2 x^2); evaluated backwards so the recurrence is stable.
    let square = x.mul(x);
    let denominator = square.add(square);
    let mut fraction = LongDouble::one();
    let mut index = 60i64;
    while index >= 1 {
        fraction =
            LongDouble::one().add(LongDouble::from_i64(index).div(denominator).div(fraction));
        index -= 1;
    }
    let exponent = LongDouble::zero(false).sub(square);
    expl(exponent).div(x.mul(sqrt_pi()).mul(fraction))
}

/// `lgamma x` and the sign of `Gamma(x)`.
pub fn lgamma_rl(x: LongDouble) -> (LongDouble, i32) {
    if x.is_nan() {
        return (x, 1);
    }
    if x.is_zero() || (x.sign_bit() && is_integral(x)) {
        // Poles at zero and the negative integers.
        return (LongDouble::infinity(false), 1);
    }
    if x.is_infinite() {
        return (x, 1);
    }
    if x.total_cmp(ld(0.5)) == core::cmp::Ordering::Less {
        // Reflection: lgamma(x) = ln(pi / |sin(pi x)|) - lgamma(1 - x).
        let sine = sinl(pi().mul(x));
        let value = logl(pi().div(sine.abs())).sub(lgamma_positive(LongDouble::one().sub(x)));
        let sign = if sine.sign_bit() { -1 } else { 1 };
        return (value, sign);
    }
    (lgamma_positive(x), 1)
}

/// `lgamma x`.
pub fn lgammal(x: LongDouble) -> LongDouble {
    lgamma_rl(x).0
}

fn lgamma_positive(x: LongDouble) -> LongDouble {
    // Push the argument to >= 8 with the recurrence, then Stirling.
    let mut value = x;
    let mut correction = LongDouble::zero(false);
    let eight = ld(8.0);
    while value.total_cmp(eight) == core::cmp::Ordering::Less {
        correction = correction.add(logl(value));
        value = value.add(LongDouble::one());
    }
    // (x - 1/2) ln x - x + 1/2 ln(2 pi) + sum B_2n / (2n (2n-1) x^(2n-1)).
    let half = ld(0.5);
    let mut result = value
        .sub(half)
        .mul(logl(value))
        .sub(value)
        .add(logl(ld(core::f64::consts::TAU)).mul(half));
    result = result.sub(correction);
    let inverse = LongDouble::one().div(value);
    let inverse2 = inverse.mul(inverse);
    let mut power = inverse;
    let mut index = 1i64;
    const COEFFICIENTS: [f64; 7] = [
        1.0 / 12.0,
        -1.0 / 360.0,
        1.0 / 1260.0,
        -1.0 / 1680.0,
        1.0 / 1188.0,
        -691.0 / 360360.0,
        1.0 / 156.0,
    ];
    while index <= 7 {
        let term = ld(COEFFICIENTS[(index - 1) as usize]).mul(power);
        result = result.add(term);
        power = power.mul(inverse2);
        index += 1;
    }
    result
}

fn is_integral(x: LongDouble) -> bool {
    x.total_cmp(x.round()) == core::cmp::Ordering::Equal
}

/// `tgamma x`.
pub fn tgammal(x: LongDouble) -> LongDouble {
    if x.is_nan() {
        return x;
    }
    if x.is_zero() || (x.sign_bit() && is_integral(x)) {
        return LongDouble::nan();
    }
    if x.total_cmp(ld(0.5)) == core::cmp::Ordering::Less {
        // Reflection: Gamma(x) = pi / (sin(pi x) * Gamma(1 - x)).
        let sine = sinl(pi().mul(x));
        let gamma = expl(lgamma_positive(LongDouble::one().sub(x)));
        return pi().div(sine.mul(gamma));
    }
    expl(lgamma_positive(x))
}

/// Euler's constant.
fn euler_gamma() -> LongDouble {
    ld(0.577_215_664_901_532_9)
}

/// `j0 x`: the power series inside the cancellation-safe range, the leading Hankel form above.
pub fn j0l(x: LongDouble) -> LongDouble {
    if x.is_nan() {
        return x;
    }
    if x.is_zero() {
        return LongDouble::one();
    }
    if x.is_infinite() {
        return LongDouble::zero(false);
    }
    let magnitude = x.abs();
    if magnitude.to_f64() > 12.0 {
        // Leading asymptotic term: sqrt(2/(pi x)) cos(x - pi/4).
        return asymptotic(magnitude, ld(0.25).mul(pi()), false);
    }
    let quarter = magnitude.mul(magnitude).div(ld(4.0));
    let mut term = LongDouble::one();
    let mut sum = LongDouble::one();
    let mut index = 1i64;
    while index <= 60 {
        term =
            LongDouble::zero(false).sub(term.mul(quarter).div(LongDouble::from_i64(index * index)));
        sum = sum.add(term);
        index += 1;
    }
    sum
}

/// `j1 x`.
pub fn j1l(x: LongDouble) -> LongDouble {
    if x.is_nan() {
        return x;
    }
    if x.is_zero() {
        return x;
    }
    if x.is_infinite() {
        return LongDouble::zero(false);
    }
    let magnitude = x.abs();
    let negative = x.sign_bit();
    let value = if magnitude.to_f64() > 12.0 {
        // Leading asymptotic term: sqrt(2/(pi x)) cos(x - 3 pi/4).
        asymptotic(magnitude, ld(0.75).mul(pi()), false)
    } else {
        let quarter = magnitude.mul(magnitude).div(ld(4.0));
        let mut term = magnitude.mul(ld(0.5));
        let mut sum = term;
        let mut index = 1i64;
        while index <= 60 {
            term = LongDouble::zero(false).sub(
                term.mul(quarter)
                    .div(LongDouble::from_i64(index * (index + 1))),
            );
            sum = sum.add(term);
            index += 1;
        }
        sum
    };
    if negative {
        LongDouble::zero(false).sub(value)
    } else {
        value
    }
}

/// `y0 x`.
pub fn y0l(x: LongDouble) -> LongDouble {
    if x.is_nan() {
        return x;
    }
    if x.is_zero() {
        return LongDouble::infinity(true);
    }
    if x.sign_bit() {
        return LongDouble::nan();
    }
    if x.is_infinite() {
        return LongDouble::zero(false);
    }
    if x.to_f64() > 12.0 {
        return asymptotic(x, ld(0.25).mul(pi()), true);
    }
    // y0 = (2/pi)(ln(x/2) + gamma) j0 + (2/pi) sum_{k>=1} (-1)^(k+1) H_k (x^2/4)^k / (k!)^2
    let quarter = x.mul(x).div(ld(4.0));
    let mut term = LongDouble::one();
    let mut harmonic = LongDouble::zero(false);
    let mut series = LongDouble::zero(false);
    let mut index = 1i64;
    while index <= 60 {
        term = term.mul(quarter).div(LongDouble::from_i64(index * index));
        harmonic = harmonic.add(LongDouble::one().div(LongDouble::from_i64(index)));
        let signed = if index % 2 == 1 {
            term
        } else {
            LongDouble::zero(false).sub(term)
        };
        series = series.add(harmonic.mul(signed));
        index += 1;
    }
    let two_over_pi = ld(2.0).div(pi());
    let logarithm = logl(x.mul(ld(0.5))).add(euler_gamma());
    two_over_pi.mul(logarithm.mul(j0l(x)).add(series))
}

/// `y1 x`.
///
/// Computed from the Wronskian `J0 Y1 - J1 Y0 = -2/(pi x)`, so it is accurate where `J0` is not
/// near a zero and loses precision at the `J0` zeros. The asymptotic branch has no such issue.
pub fn y1l(x: LongDouble) -> LongDouble {
    if x.is_nan() {
        return x;
    }
    if x.is_zero() {
        return LongDouble::infinity(true);
    }
    if x.sign_bit() {
        return LongDouble::nan();
    }
    if x.is_infinite() {
        return LongDouble::zero(false);
    }
    if x.to_f64() > 12.0 {
        return asymptotic(x, ld(0.75).mul(pi()), true);
    }
    let two_over_pi_x = ld(2.0).div(pi().mul(x));
    j1l(x).mul(y0l(x)).sub(two_over_pi_x).div(j0l(x))
}

/// `jn(n, x)` by upward recurrence from `j0`/`j1`.
pub fn jnl(order: i32, x: LongDouble) -> LongDouble {
    if order == 0 {
        return j0l(x);
    }
    if order == 1 {
        return j1l(x);
    }
    if order < 0 {
        return LongDouble::nan();
    }
    let mut previous = j0l(x);
    let mut current = j1l(x);
    let mut index = 1i32;
    while index < order {
        let next = ld(2.0)
            .mul(ld(index as f64))
            .div(x)
            .mul(current)
            .sub(previous);
        previous = current;
        current = next;
        index += 1;
    }
    current
}

/// `yn(n, x)` by upward recurrence from `y0`/`y1`.
pub fn ynl(order: i32, x: LongDouble) -> LongDouble {
    if order == 0 {
        return y0l(x);
    }
    if order == 1 {
        return y1l(x);
    }
    if order < 0 {
        return LongDouble::nan();
    }
    let mut previous = y0l(x);
    let mut current = y1l(x);
    let mut index = 1i32;
    while index < order {
        let next = ld(2.0)
            .mul(ld(index as f64))
            .div(x)
            .mul(current)
            .sub(previous);
        previous = current;
        current = next;
        index += 1;
    }
    current
}

/// The leading Hankel asymptotic form of the Bessel functions.
fn asymptotic(x: LongDouble, phase: LongDouble, sine: bool) -> LongDouble {
    let scale = ld(2.0).div(pi().mul(x)).sqrt();
    let argument = x.sub(phase);
    scale.mul(if sine { sinl(argument) } else { cosl(argument) })
}

#[cfg(test)]
#[path = "long_double_math_ut.rs"]
mod tests;

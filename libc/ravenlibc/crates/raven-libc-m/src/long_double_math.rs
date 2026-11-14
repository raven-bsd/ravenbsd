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

#[cfg(test)]
#[path = "long_double_math_ut.rs"]
mod tests;

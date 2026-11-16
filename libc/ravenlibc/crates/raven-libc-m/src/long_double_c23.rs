// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The remaining `long double` entry points that are not exact-class bit copies.
//!
//! These are computed with the soft `LongDouble` arithmetic of [`crate::long_double`]; the
//! transcendentals (`expl`, `logl`, `sinl`, ...) live in [`crate::long_double_math`]. The
//! `fmal` entry is a two-rounding multiply-add (the soft type has no wide accumulator); its
//! limitation is documented on the item.

use crate::LongDouble;

/// `cbrtl`: the cube root, Newton iterations on a scaled initial guess.
pub fn cbrtl(x: LongDouble) -> LongDouble {
    if x.is_nan() || x.is_infinite() || x.is_zero() {
        return x;
    }
    let a = x.abs();
    let (m, e) = frexpl(a);
    let k = e.rem_euclid(3);
    let scaled = m.mul(LongDouble::one().scalbn(k));
    let mut y = LongDouble::from_f64(crate::cbrt(scaled.to_f64()));
    let three = LongDouble::from_f64(3.0);
    for _ in 0..4 {
        y = y.add(y).add(scaled.div(y.mul(y))).div(three);
    }
    y.mul(LongDouble::one().scalbn((e - k) / 3)).copysign(x)
}

/// `fmal`: `x * y + z` with the soft long-double arithmetic.
///
/// The product is rounded once and the sum once; this is not a single-rounding fused operation.
pub fn fmal(x: LongDouble, y: LongDouble, z: LongDouble) -> LongDouble {
    x.mul(y).add(z)
}

/// `frexpl`: split into a fraction in `[0.5, 1)` and a power of two.
pub fn frexpl(x: LongDouble) -> (LongDouble, i32) {
    if x.is_nan() || x.is_infinite() || x.is_zero() {
        return (x, 0);
    }
    if x.is_normal() {
        let exponent = x.exponent_field() as i32 - 16383;
        let mantissa = LongDouble::from_parts(x.sign_bit(), 16382, x.significand_bits());
        return (mantissa, exponent + 1);
    }
    let scaled = x.scalbn(64);
    let (mantissa, exponent) = frexpl(scaled);
    (mantissa, exponent - 64)
}

/// `hypotl`: `sqrt(x*x + y*y)` with scaling against overflow.
pub fn hypotl(x: LongDouble, y: LongDouble) -> LongDouble {
    let ax = x.abs();
    let ay = y.abs();
    if ax.is_infinite() || ay.is_infinite() {
        return LongDouble::infinity(false);
    }
    if x.is_nan() || y.is_nan() {
        return x.add(y);
    }
    if ax.is_zero() {
        return ay;
    }
    if ay.is_zero() {
        return ax;
    }
    let larger = match ax.partial_cmp(ay) {
        Some(core::cmp::Ordering::Less) => ay,
        _ => ax,
    };
    let (_, exponent) = frexpl(larger);
    let scale = LongDouble::one().scalbn(exponent);
    let sx = ax.div(scale);
    let sy = ay.div(scale);
    sx.mul(sx).add(sy.mul(sy)).sqrt().mul(scale)
}

/// `ilogbl`: the binary exponent as a C `int`.
pub fn ilogbl(x: LongDouble) -> i32 {
    if x.is_nan() || x.is_zero() {
        return i32::MIN;
    }
    if x.is_infinite() {
        return i32::MAX;
    }
    if x.is_normal() {
        return x.exponent_field() as i32 - 16383;
    }
    let (_, exponent) = frexpl(x);
    exponent - 1
}

/// `logbl`: the binary exponent as a `long double`.
pub fn logbl(x: LongDouble) -> LongDouble {
    if x.is_nan() {
        return x;
    }
    if x.is_infinite() {
        return LongDouble::infinity(false);
    }
    if x.is_zero() {
        return LongDouble::infinity(true);
    }
    LongDouble::from_i64(i64::from(ilogbl(x)))
}

/// `llogbl`: the binary exponent as a C `long`.
pub fn llogbl(x: LongDouble) -> i64 {
    if x.is_nan() || x.is_zero() {
        return i64::MIN;
    }
    if x.is_infinite() {
        return i64::MAX;
    }
    i64::from(ilogbl(x))
}

/// `ldexpl`: `x * 2^n`.
pub fn ldexpl(x: LongDouble, n: i32) -> LongDouble {
    x.scalbn(n)
}

/// `scalblnl`: `x * 2^n` with a `long` exponent.
pub fn scalblnl(x: LongDouble, n: i64) -> LongDouble {
    if n > i64::from(i32::MAX) {
        return x.scalbn(i32::MAX);
    }
    if n < i64::from(i32::MIN) {
        return x.scalbn(i32::MIN);
    }
    x.scalbn(n as i32)
}

/// `modfl`: the fractional and integral parts.
pub fn modfl(x: LongDouble) -> (LongDouble, LongDouble) {
    if !x.is_finite() {
        return (LongDouble::zero(x.sign_bit()), x);
    }
    let integral = x.trunc();
    (x.sub(integral), integral)
}

/// `sincosl`: `sinl(x)` and `cosl(x)` together.
pub fn sincosl(x: LongDouble) -> (LongDouble, LongDouble) {
    (
        crate::long_double_math::sinl(x),
        crate::long_double_math::cosl(x),
    )
}

/// `significandl`: `x` scaled to `[1, 2)`.
pub fn significandl(x: LongDouble) -> LongDouble {
    if x.is_nan() || x.is_infinite() || x.is_zero() {
        return x;
    }
    x.scalbn(-ilogbl(x))
}

/// `pow10l`: the legacy alias of `exp10l`.
pub fn pow10l(x: LongDouble) -> LongDouble {
    crate::long_double_math::exp10l(x)
}

/// `nanl` with the C tag string turned into a quiet-NaN payload.
pub fn nanl_with_tag(tag: &[u8]) -> LongDouble {
    let mut digits = tag;
    if digits.len() >= 2 && digits[0] == b'0' && (digits[1] == b'x' || digits[1] == b'X') {
        digits = digits.get(2..).unwrap_or(&[]);
    }
    let mut payload = 0u64;
    for byte in digits {
        let value = match *byte {
            b'0'..=b'9' => u64::from(*byte - b'0'),
            b'a'..=b'f' => u64::from(*byte - b'a') + 10,
            b'A'..=b'F' => u64::from(*byte - b'A') + 10,
            _ => break,
        };
        payload = (payload << 4) | value;
    }
    let quiet = (1u64 << 63) | (1u64 << 62);
    LongDouble::from_parts(false, 0x7fff, quiet | (payload & ((1u64 << 62) - 1)))
}

#[cfg(test)]
#[path = "long_double_c23_ut.rs"]
mod tests;

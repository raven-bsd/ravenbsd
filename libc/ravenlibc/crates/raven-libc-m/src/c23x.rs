// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C23/C2x scalar additions and the deprecated magnitude aliases.
//!
//! These are first-party implementations over the exact-class operations in this crate. The
//! rounding, classification and bit-stepping entries follow the IEEE 754 / ISO C23 rules; the
//! algebraic entries (`pown`, `rootn`, `powr`, `compoundn`, the `pi` family and the
//! `exp10m1`/`log10p1` family) are built from the correctly rounded base functions and document
//! their accuracy limits on the individual items.

use core::cmp::Ordering;

use crate::LongDouble;
use crate::consts::{FP_SUBNORMAL, FP_ZERO};

const SIGN64: u64 = 1 << 63;
const SIGN32: u32 = 1 << 31;

/// The C23 `FP_INT_*` rounding selections for `fromfp`/`fromfpx`.
pub const FP_INT_UPWARD: i32 = 0;
/// Round toward `-infinity`.
pub const FP_INT_DOWNWARD: i32 = 1;
/// Round toward zero.
pub const FP_INT_TOWARDZERO: i32 = 2;
/// Round to nearest, ties away from zero.
pub const FP_INT_TONEARESTFROMZERO: i32 = 3;
/// Round to nearest, ties to even.
pub const FP_INT_TONEAREST: i32 = 4;

fn step_up_64(x: f64) -> f64 {
    if x == 0.0 {
        return f64::from_bits(1);
    }
    if x > 0.0 {
        f64::from_bits(x.to_bits() + 1)
    } else {
        f64::from_bits(x.to_bits() - 1)
    }
}

fn step_down_64(x: f64) -> f64 {
    if x == 0.0 {
        return -f64::from_bits(1);
    }
    if x > 0.0 {
        f64::from_bits(x.to_bits() - 1)
    } else {
        f64::from_bits(x.to_bits() + 1)
    }
}

fn step_up_32(x: f32) -> f32 {
    if x == 0.0 {
        return f32::from_bits(1);
    }
    if x > 0.0 {
        f32::from_bits(x.to_bits() + 1)
    } else {
        f32::from_bits(x.to_bits() - 1)
    }
}

fn step_down_32(x: f32) -> f32 {
    if x == 0.0 {
        return -f32::from_bits(1);
    }
    if x > 0.0 {
        f32::from_bits(x.to_bits() - 1)
    } else {
        f32::from_bits(x.to_bits() + 1)
    }
}

/// `nextafter` for `f64`.
pub fn nextafter(x: f64, y: f64) -> f64 {
    if x.is_nan() || y.is_nan() {
        return x + y;
    }
    if x == y {
        return y;
    }
    if x == 0.0 {
        return f64::from_bits((y.to_bits() & SIGN64) | 1);
    }
    let up = if x > 0.0 { y > x } else { y < x };
    if up { step_up_64(x) } else { step_down_64(x) }
}

/// `nextafter` for `f32`.
pub fn nextafterf(x: f32, y: f32) -> f32 {
    if x.is_nan() || y.is_nan() {
        return x + y;
    }
    if x == y {
        return y;
    }
    if x == 0.0 {
        return f32::from_bits((y.to_bits() & SIGN32) | 1);
    }
    let up = if x > 0.0 { y > x } else { y < x };
    if up { step_up_32(x) } else { step_down_32(x) }
}

/// `nexttoward`: the direction is taken in `long double`, the step is an `f64` ULP.
pub fn nexttoward(x: f64, y: LongDouble) -> f64 {
    if x.is_nan() {
        return x;
    }
    if y.is_nan() {
        return f64::NAN;
    }
    let xl = LongDouble::from_f64(x);
    match y.partial_cmp(xl) {
        Some(Ordering::Equal) => x,
        Some(Ordering::Greater) => {
            if x == 0.0 {
                f64::from_bits(1)
            } else {
                step_up_64(x)
            }
        }
        Some(Ordering::Less) => {
            if x == 0.0 {
                -f64::from_bits(1)
            } else {
                step_down_64(x)
            }
        }
        None => x,
    }
}

/// `nexttoward` for `f32`.
pub fn nexttowardf(x: f32, y: LongDouble) -> f32 {
    if x.is_nan() {
        return x;
    }
    if y.is_nan() {
        return f32::NAN;
    }
    let xl = LongDouble::from_f64(f64::from(x));
    match y.partial_cmp(xl) {
        Some(Ordering::Equal) => x,
        Some(Ordering::Greater) => {
            if x == 0.0 {
                f32::from_bits(1)
            } else {
                step_up_32(x)
            }
        }
        Some(Ordering::Less) => {
            if x == 0.0 {
                -f32::from_bits(1)
            } else {
                step_down_32(x)
            }
        }
        None => x,
    }
}

/// `logb`: the binary exponent of `x` as a floating value.
pub fn logb(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    if x.is_infinite() {
        return f64::INFINITY;
    }
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }
    f64::from(crate::ilogb(x))
}

/// `logb` for `f32`.
pub fn logbf(x: f32) -> f32 {
    if x.is_nan() {
        return x;
    }
    if x.is_infinite() {
        return f32::INFINITY;
    }
    if x == 0.0 {
        return f32::NEG_INFINITY;
    }
    crate::ilogbf(x) as f32
}

/// `llogb`: the binary exponent as a C `long` (64-bit here).
pub fn llogb(x: f64) -> i64 {
    if x.is_nan() || x == 0.0 {
        return i64::MIN;
    }
    if x.is_infinite() {
        return i64::MAX;
    }
    i64::from(crate::ilogb(x))
}

/// `scalbln`: `x * 2^n` with a `long` exponent.
pub fn scalbln(x: f64, n: i64) -> f64 {
    if n > i64::from(i32::MAX) {
        return crate::scalbn(x, i32::MAX);
    }
    if n < i64::from(i32::MIN) {
        return crate::scalbn(x, i32::MIN);
    }
    crate::scalbn(x, n as i32)
}

/// `scalbln` for `f32`.
pub fn scalblnf(x: f32, n: i64) -> f32 {
    if n > i64::from(i32::MAX) {
        return crate::scalbnf(x, i32::MAX);
    }
    if n < i64::from(i32::MIN) {
        return crate::scalbnf(x, i32::MIN);
    }
    crate::scalbnf(x, n as i32)
}

/// `lrint`: round to the current integer, return a C `long`.
pub fn lrint(x: f64) -> i64 {
    crate::rint(x) as i64
}

/// `lrint` for `f32`.
pub fn lrintf(x: f32) -> i64 {
    crate::rintf(x) as i64
}

/// `llrint`: round to the current integer, return a C `long long`.
pub fn llrint(x: f64) -> i64 {
    crate::rint(x) as i64
}

/// `llrint` for `f32`.
pub fn llrintf(x: f32) -> i64 {
    crate::rintf(x) as i64
}

/// `lround`: round halfway away from zero, return a C `long`.
pub fn lround(x: f64) -> i64 {
    crate::round(x) as i64
}

/// `lround` for `f32`.
pub fn lroundf(x: f32) -> i64 {
    crate::roundf(x) as i64
}

/// `llround`: round halfway away from zero, return a C `long long`.
pub fn llround(x: f64) -> i64 {
    crate::round(x) as i64
}

/// `llround` for `f32`.
pub fn llroundf(x: f32) -> i64 {
    crate::roundf(x) as i64
}

/// `nearbyint`: round to an integer without the inexact flag.
pub fn nearbyint(x: f64) -> f64 {
    crate::rint(x)
}

/// `nearbyint` for `f32`.
pub fn nearbyintf(x: f32) -> f32 {
    crate::rintf(x)
}

fn quotient_is_odd(x: f64, remainder: f64, y: f64) -> bool {
    let quotient = (x - remainder) / y;
    if !quotient.is_finite() || quotient.abs() >= 9_007_199_254_740_992.0 {
        return false;
    }
    (quotient as i64) & 1 != 0
}

fn remainder_from(r: f64, x: f64, y: f64) -> f64 {
    if r == 0.0 {
        return r;
    }
    let ay = crate::fabs(y);
    let ar = crate::fabs(r);
    let mut result = r;
    if 2.0 * ar > ay || (2.0 * ar == ay && quotient_is_odd(x, r, y)) {
        result = r - crate::copysign(ay, r);
    }
    if result == 0.0 {
        return crate::copysign(0.0, x);
    }
    result
}

/// `remainder`: the IEEE remainder of `x / y`.
pub fn remainder(x: f64, y: f64) -> f64 {
    let r = crate::fmod(x, y);
    remainder_from(r, x, y)
}

/// `remainder` for `f32`.
pub fn remainderf(x: f32, y: f32) -> f32 {
    let r = crate::fmodf(x, y);
    if r == 0.0 || r.is_nan() {
        return r;
    }
    let ay = crate::fabsf(y);
    let ar = crate::fabsf(r);
    let mut result = r;
    if 2.0 * ar > ay {
        result = r - crate::copysignf(ay, r);
    } else if 2.0 * ar == ay {
        let quotient = (x - r) / y;
        if quotient.abs() < 8_388_608.0 && (quotient as i64) & 1 != 0 {
            result = r - crate::copysignf(ay, r);
        }
    }
    if result == 0.0 {
        crate::copysignf(0.0, x)
    } else {
        result
    }
}

/// `remquo`: the IEEE remainder plus the low bits of the quotient.
pub fn remquo(x: f64, y: f64, quo: &mut i32) -> f64 {
    let r = crate::fmod(x, y);
    let result = remainder_from(r, x, y);
    let quotient = if result == 0.0 {
        if x == 0.0 { 0.0 } else { x / y }
    } else {
        (x - result) / y
    };
    let magnitude = if quotient.is_finite() && quotient.abs() < 9_223_372_036_854_775_808.0 {
        (quotient as i64).unsigned_abs() & 0x7f
    } else {
        0
    };
    let signed = i32::try_from(magnitude).unwrap_or(0);
    *quo = if (x < 0.0) != (y < 0.0) {
        -signed
    } else {
        signed
    };
    result
}

/// `remquo` for `f32`.
pub fn remquof(x: f32, y: f32, quo: &mut i32) -> f32 {
    let result = remainderf(x, y);
    let quotient = if result == 0.0 {
        if x == 0.0 { 0.0 } else { x / y }
    } else {
        (x - result) / y
    };
    let magnitude = if quotient.is_finite() && quotient.abs() < 8_388_608.0 {
        (quotient as i64).unsigned_abs() & 0x7f
    } else {
        0
    };
    let signed = i32::try_from(magnitude).unwrap_or(0);
    *quo = if (x < 0.0) != (y < 0.0) {
        -signed
    } else {
        signed
    };
    result
}

/// C23 `fmaximum_mag` (deprecated name `fmaxmag`).
pub fn fmaxmag(x: f64, y: f64) -> f64 {
    crate::fmaximum_mag(x, y)
}

/// `fmaxmag` for `f32`.
pub fn fmaxmagf(x: f32, y: f32) -> f32 {
    crate::fmaximum_magf(x, y)
}

/// C23 `fminimum_mag` (deprecated name `fminmag`).
pub fn fminmag(x: f64, y: f64) -> f64 {
    crate::fminimum_mag(x, y)
}

/// `fminmag` for `f32`.
pub fn fminmagf(x: f32, y: f32) -> f32 {
    crate::fminimum_magf(x, y)
}

/// C23 `fmaximum_mag_num`: the NaN-ignoring form.
pub fn fmaximum_mag_num(x: f64, y: f64) -> f64 {
    crate::fmaximum_mag(x, y)
}

/// `fmaximum_mag_num` for `f32`.
pub fn fmaximum_mag_numf(x: f32, y: f32) -> f32 {
    crate::fmaximum_magf(x, y)
}

/// C23 `fminimum_mag_num`: the NaN-ignoring form.
pub fn fminimum_mag_num(x: f64, y: f64) -> f64 {
    crate::fminimum_mag(x, y)
}

/// `fminimum_mag_num` for `f32`.
pub fn fminimum_mag_numf(x: f32, y: f32) -> f32 {
    crate::fminimum_magf(x, y)
}

/// `pown`: `x` raised to the integer power `n`.
pub fn pown(x: f64, n: i32) -> f64 {
    if n == 0 {
        return 1.0;
    }
    let mut base = if n < 0 { 1.0 / x } else { x };
    let mut exponent = n.unsigned_abs();
    let mut result = 1.0;
    while exponent != 0 {
        if exponent & 1 == 1 {
            result *= base;
        }
        base *= base;
        exponent >>= 1;
    }
    result
}

/// `pown` for `f32`.
pub fn pownf(x: f32, n: i32) -> f32 {
    if n == 0 {
        return 1.0;
    }
    let mut base = if n < 0 { 1.0 / x } else { x };
    let mut exponent = n.unsigned_abs();
    let mut result = 1.0;
    while exponent != 0 {
        if exponent & 1 == 1 {
            result *= base;
        }
        base *= base;
        exponent >>= 1;
    }
    result
}

/// `rootn`: the real `n`-th root of `x`.
pub fn rootn(x: f64, n: i32) -> f64 {
    if n == 0 {
        return f64::NAN;
    }
    if x.is_nan() {
        return x;
    }
    if n == 1 {
        return x;
    }
    if n == -1 {
        return 1.0 / x;
    }
    let odd = n & 1 != 0;
    let magnitude = n.unsigned_abs();
    if x < 0.0 && !odd {
        return f64::NAN;
    }
    let root = if x < 0.0 {
        -crate::pow(-x, 1.0 / f64::from(magnitude))
    } else {
        crate::pow(x, 1.0 / f64::from(magnitude))
    };
    if n < 0 { 1.0 / root } else { root }
}

/// `rootn` for `f32`.
pub fn rootnf(x: f32, n: i32) -> f32 {
    if n == 0 {
        return f32::NAN;
    }
    if x.is_nan() {
        return x;
    }
    if n == 1 {
        return x;
    }
    if n == -1 {
        return 1.0 / x;
    }
    let odd = n & 1 != 0;
    let magnitude = n.unsigned_abs();
    if x < 0.0 && !odd {
        return f32::NAN;
    }
    let root = if x < 0.0 {
        -crate::powf(-x, 1.0 / magnitude as f32)
    } else {
        crate::powf(x, 1.0 / magnitude as f32)
    };
    if n < 0 { 1.0 / root } else { root }
}

/// `powr`: `x^y` with a NaN domain for negative `x`.
pub fn powr(x: f64, y: f64) -> f64 {
    if x < 0.0 {
        return f64::NAN;
    }
    crate::pow(x, y)
}

/// `powr` for `f32`.
pub fn powrf(x: f32, y: f32) -> f32 {
    if x < 0.0 {
        return f32::NAN;
    }
    crate::powf(x, y)
}

/// `compoundn`: `(1 + x)^n`, built with `log1p`/`exp`.
pub fn compoundn(x: f64, n: i64) -> f64 {
    if x < -1.0 {
        return f64::NAN;
    }
    if x == -1.0 {
        return if n > 0 { 0.0 } else { f64::INFINITY };
    }
    let exponent = n as f64;
    if x.abs() < 1.0e-4 {
        crate::exp(exponent * crate::log1p(x))
    } else {
        crate::pow(1.0 + x, exponent)
    }
}

/// `compoundn` for `f32`.
pub fn compoundnf(x: f32, n: i64) -> f32 {
    if x < -1.0 {
        return f32::NAN;
    }
    if x == -1.0 {
        return if n > 0 { 0.0 } else { f32::INFINITY };
    }
    let exponent = n as f32;
    if x.abs() < 1.0e-4 {
        crate::expf(exponent * crate::log1pf(x))
    } else {
        crate::powf(1.0 + x, exponent)
    }
}

const LN_10: f64 = core::f64::consts::LN_10;
const LN_2: f64 = core::f64::consts::LN_2;
const LOG10_E: f64 = core::f64::consts::LOG10_E;
const LOG2_E: f64 = core::f64::consts::LOG2_E;

/// `exp10m1`: `10^x - 1`, accurate near zero.
pub fn exp10m1(x: f64) -> f64 {
    crate::expm1(x * LN_10)
}

/// `exp10m1` for `f32`.
pub fn exp10m1f(x: f32) -> f32 {
    crate::expm1f(x * LN_10 as f32)
}

/// `exp2m1`: `2^x - 1`, accurate near zero.
pub fn exp2m1(x: f64) -> f64 {
    crate::expm1(x * LN_2)
}

/// `exp2m1` for `f32`.
pub fn exp2m1f(x: f32) -> f32 {
    crate::expm1f(x * LN_2 as f32)
}

/// `log10p1`: `log10(1 + x)`, accurate near zero.
pub fn log10p1(x: f64) -> f64 {
    crate::log1p(x) * LOG10_E
}

/// `log10p1` for `f32`.
pub fn log10p1f(x: f32) -> f32 {
    crate::log1pf(x) * LOG10_E as f32
}

/// `log2p1`: `log2(1 + x)`, accurate near zero.
pub fn log2p1(x: f64) -> f64 {
    crate::log1p(x) * LOG2_E
}

/// `log2p1` for `f32`.
pub fn log2p1f(x: f32) -> f32 {
    crate::log1pf(x) * LOG2_E as f32
}

/// `cospi`: `cos(pi * x)` without the rounding of `pi * x`.
pub fn cospi(x: f64) -> f64 {
    crate::sinpi(0.5 - x)
}

/// `tanpi`: `tan(pi * x)`.
pub fn tanpi(x: f64) -> f64 {
    crate::sinpi(x) / cospi(x)
}

/// `asinpi`: `asin(x) / pi`.
pub fn asinpi(x: f64) -> f64 {
    crate::asin(x) / core::f64::consts::PI
}

/// `acospi`: `acos(x) / pi`.
pub fn acospi(x: f64) -> f64 {
    crate::acos(x) / core::f64::consts::PI
}

/// `atanpi`: `atan(x) / pi`.
pub fn atanpi(x: f64) -> f64 {
    crate::atan(x) / core::f64::consts::PI
}

/// `atan2pi`: `atan2(y, x) / pi`.
pub fn atan2pi(y: f64, x: f64) -> f64 {
    crate::atan2(y, x) / core::f64::consts::PI
}

fn round_for_mode(x: f64, mode: i32) -> f64 {
    match mode {
        FP_INT_UPWARD => crate::ceil(x),
        FP_INT_DOWNWARD => crate::floor(x),
        FP_INT_TOWARDZERO => crate::trunc(x),
        FP_INT_TONEARESTFROMZERO => crate::round(x),
        _ => crate::rint(x),
    }
}

fn fits_signed(value: f64, width: u32) -> bool {
    if width == 0 || width >= 64 {
        return width != 0;
    }
    let limit = (1u64 << (width - 1)) as f64;
    value >= -limit && value < limit
}

fn fits_unsigned(value: f64, width: u32) -> bool {
    if width == 0 {
        return false;
    }
    if width >= 64 {
        return true;
    }
    let limit = (1u64 << width) as f64;
    value >= 0.0 && value < limit
}

/// `fromfp`: round to an integer selected by `mode` and `width` bits.
pub fn fromfp(x: f64, mode: i32, width: u32) -> i64 {
    if x.is_nan() || x.is_infinite() {
        return 0;
    }
    let rounded = round_for_mode(x, mode);
    if !fits_signed(rounded, width) {
        return 0;
    }
    rounded as i64
}

/// `fromfpx`: as `fromfp` (the inexact flag is not simulated).
pub fn fromfpx(x: f64, mode: i32, width: u32) -> i64 {
    fromfp(x, mode, width)
}

/// `ufromfp`: the unsigned form of `fromfp`.
pub fn ufromfp(x: f64, mode: i32, width: u32) -> u64 {
    if x.is_nan() || x.is_infinite() {
        return 0;
    }
    let rounded = round_for_mode(x, mode);
    if !fits_unsigned(rounded, width) {
        return 0;
    }
    rounded as u64
}

/// `ufromfpx`: as `ufromfp` (the inexact flag is not simulated).
pub fn ufromfpx(x: f64, mode: i32, width: u32) -> u64 {
    ufromfp(x, mode, width)
}

/// `fpclassify` as a function symbol (the header macro is the normal entry).
pub fn fpclassify(x: f64) -> i32 {
    crate::fp::fpclassify(x)
}

/// `isfinite` as a function symbol.
pub fn isfinite(x: f64) -> i32 {
    i32::from(crate::fp::isfinite(x))
}

/// `isnan` as a function symbol.
pub fn isnan(x: f64) -> i32 {
    i32::from(crate::fp::isnan(x))
}

/// `isnormal` as a function symbol.
pub fn isnormal(x: f64) -> i32 {
    i32::from(crate::fp::isnormal(x))
}

/// `signbit` as a function symbol.
pub fn signbit(x: f64) -> i32 {
    i32::from(crate::fp::signbit(x))
}

/// `isgreater` as a function symbol.
pub fn isgreater(x: f64, y: f64) -> i32 {
    i32::from(x > y)
}

/// `isgreaterequal` as a function symbol.
pub fn isgreaterequal(x: f64, y: f64) -> i32 {
    i32::from(x >= y)
}

/// `isless` as a function symbol.
pub fn isless(x: f64, y: f64) -> i32 {
    i32::from(x < y)
}

/// `islessequal` as a function symbol.
pub fn islessequal(x: f64, y: f64) -> i32 {
    i32::from(x <= y)
}

/// `islessgreater` as a function symbol.
pub fn islessgreater(x: f64, y: f64) -> i32 {
    i32::from(x != y)
}

/// `isunordered` as a function symbol.
pub fn isunordered(x: f64, y: f64) -> i32 {
    i32::from(x.is_nan() || y.is_nan())
}

/// `iseqsig` as a function symbol.
pub fn iseqsig(x: f64, y: f64) -> i32 {
    i32::from(x == y)
}

/// `iszero` as a function symbol.
pub fn iszero(x: f64) -> i32 {
    i32::from(crate::fp::fpclassify(x) == FP_ZERO)
}

/// `issubnormal` as a function symbol.
pub fn issubnormal(x: f64) -> i32 {
    i32::from(crate::fp::fpclassify(x) == FP_SUBNORMAL)
}

/// `iscanonical`: every binary64 value is canonical.
pub fn iscanonical(_x: f64) -> i32 {
    1
}

#[cfg(test)]
#[path = "c23x_ut.rs"]
mod tests;

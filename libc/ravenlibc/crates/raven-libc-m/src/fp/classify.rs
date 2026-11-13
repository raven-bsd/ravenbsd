// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `fpclassify`, `isnan`, `isinf`, `isfinite`, `isnormal` and `signbit` for `f32` and `f64`.
//!
//! Ten-line bit predicates from IEEE 754. The `long double` predicates are part of the phase-4
//! slice and are not present yet.

use crate::consts::{FP_INFINITE, FP_NAN, FP_NORMAL, FP_SUBNORMAL, FP_ZERO};

const F32_EXP_MASK: u32 = 0x7f80_0000;
const F32_MANT_MASK: u32 = 0x007f_ffff;
const F32_SIGN: u32 = 0x8000_0000;

const F64_EXP_MASK: u64 = 0x7ff0_0000_0000_0000;
const F64_MANT_MASK: u64 = 0x000f_ffff_ffff_ffff;
const F64_SIGN: u64 = 0x8000_0000_0000_0000;

/// The `FP_*` class of an `f64`.
#[inline]
pub fn fpclassify(x: f64) -> i32 {
    let bits = x.to_bits();
    let exponent = bits & F64_EXP_MASK;
    let mantissa = bits & F64_MANT_MASK;
    if exponent == F64_EXP_MASK {
        if mantissa == 0 { FP_INFINITE } else { FP_NAN }
    } else if exponent == 0 {
        if mantissa == 0 { FP_ZERO } else { FP_SUBNORMAL }
    } else {
        FP_NORMAL
    }
}

/// The `FP_*` class of an `f32`.
#[inline]
pub fn fpclassifyf(x: f32) -> i32 {
    let bits = x.to_bits();
    let exponent = bits & F32_EXP_MASK;
    let mantissa = bits & F32_MANT_MASK;
    if exponent == F32_EXP_MASK {
        if mantissa == 0 { FP_INFINITE } else { FP_NAN }
    } else if exponent == 0 {
        if mantissa == 0 { FP_ZERO } else { FP_SUBNORMAL }
    } else {
        FP_NORMAL
    }
}

/// Whether `x` is NaN (any payload, quiet or signalling).
#[inline]
pub fn isnan(x: f64) -> bool {
    x.to_bits() & !F64_SIGN > F64_EXP_MASK
}

/// Whether an `f32` is NaN.
#[inline]
pub fn isnanf(x: f32) -> bool {
    x.to_bits() & !F32_SIGN > F32_EXP_MASK
}

/// Whether `x` is positive or negative infinity.
#[inline]
pub fn isinf(x: f64) -> bool {
    x.to_bits() & !F64_SIGN == F64_EXP_MASK
}

/// Whether an `f32` is infinity.
#[inline]
pub fn isinff(x: f32) -> bool {
    x.to_bits() & !F32_SIGN == F32_EXP_MASK
}

/// Whether `x` is finite (neither NaN nor infinity).
#[inline]
pub fn isfinite(x: f64) -> bool {
    !isnan(x) && !isinf(x)
}

/// Whether an `f32` is finite.
#[inline]
pub fn isfinitef(x: f32) -> bool {
    !isnanf(x) && !isinff(x)
}

/// Whether `x` is a normal number (not zero, subnormal, infinity or NaN).
#[inline]
pub fn isnormal(x: f64) -> bool {
    fpclassify(x) == FP_NORMAL
}

/// Whether an `f32` is normal.
#[inline]
pub fn isnormalf(x: f32) -> bool {
    fpclassifyf(x) == FP_NORMAL
}

/// Whether the sign bit of `x` is set (also true for `-0` and negative NaNs).
#[inline]
pub fn signbit(x: f64) -> bool {
    x.to_bits() & F64_SIGN != 0
}

/// Whether the sign bit of an `f32` is set.
#[inline]
pub fn signbitf(x: f32) -> bool {
    x.to_bits() & F32_SIGN != 0
}

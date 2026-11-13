// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `sinpi`: `sin(pi * x)` without the rounding of `pi * x`.
//!
//! Functional specification read from the FreeBSD `lib/msun` `s_sinpi.c` comments and behaviour
//! (BSD-2-Clause, tier A); this implementation is written for RavenBSD from that specification,
//! not copied. Donor semantics:
//!
//! * `sinpi(+-0) = +-0`, `sinpi(+-n) = +-0` for positive integers `n`;
//! * `sinpi(+-inf) = NaN` (invalid), `sinpi(NaN) = NaN`;
//! * for `|x| >= 2^52` the argument is integral, so the result is `copysign(0, x)`;
//! * otherwise the fractional part is reduced exactly and the identity
//!   `sin(pi*(n + r)) = (-1)^n * sin(pi*r)` is evaluated with the host `sin`.
//!
//! The reduction is exact (the fractional part of a double below `2^52` is exact) and the final
//! `sin` is the correctly rounded f64 one, so the result is within one ULP of the exact value;
//! the donor's kernel split reaches the same bound.

use core::f64::consts::PI;

/// `sin(pi * x)`.
#[inline]
pub fn sinpi(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    if x.is_infinite() {
        return f64::NAN;
    }
    if x == 0.0 {
        return x;
    }
    let sign = if x.is_sign_negative() { -1.0 } else { 1.0 };
    let magnitude = x.abs();
    if magnitude >= 4_503_599_627_370_496.0 {
        // 2^52: every value is integral, so sin(pi*x) is a signed zero.
        return sign * 0.0;
    }
    let whole = crate::trunc(magnitude);
    let fraction = magnitude - whole;
    if fraction == 0.0 {
        return sign * 0.0;
    }
    let reduced = if fraction == 0.5 {
        sign
    } else {
        sign * crate::sin(PI * fraction)
    };
    if (whole % 2.0) == 1.0 {
        -reduced
    } else {
        reduced
    }
}

/// `sin(pi * x)` for f32.
#[inline]
pub fn sinpif(x: f32) -> f32 {
    if x.is_nan() {
        return x;
    }
    if x.is_infinite() {
        return f32::NAN;
    }
    if x == 0.0 {
        return x;
    }
    let sign = if x.is_sign_negative() { -1.0f32 } else { 1.0 };
    let magnitude = x.abs();
    let whole = crate::truncf(magnitude);
    let fraction = magnitude - whole;
    if fraction == 0.0 {
        return sign * 0.0;
    }
    let reduced = if fraction == 0.5 {
        sign
    } else {
        sign * crate::sinf(core::f32::consts::PI * fraction)
    };
    if (whole % 2.0) == 1.0 {
        -reduced
    } else {
        reduced
    }
}

#[cfg(test)]
#[path = "sinpi_ut.rs"]
mod tests;

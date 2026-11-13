// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The BSD/GNU legacy names kept as thin aliases of the C99 functions.
//!
//! These are the names that old BSD and GNU sources use; each one is the same operation as the
//! standard entry point, documented at the alias. Written for RavenBSD from the historical
//! declarations (FreeBSD `math.h`, GNU `math.h`), not copied.

/// BSD `sincos(x)`: sine and cosine in one call.
pub fn sincos(x: f64) -> (f64, f64) {
    (crate::sin(x), crate::cos(x))
}

/// BSD `sincosf(x)`.
pub fn sincosf(x: f32) -> (f32, f32) {
    (crate::sinf(x), crate::cosf(x))
}

/// GNU `pow10(x)`: the same as C23 `exp10`.
pub fn pow10(x: f64) -> f64 {
    crate::exp10(x)
}

/// GNU `pow10f(x)`.
pub fn pow10f(x: f32) -> f32 {
    crate::exp10f(x)
}

/// BSD `finite(x)`: whether `x` is neither infinite nor NaN.
pub fn finite(x: f64) -> bool {
    crate::isfinite(x)
}

/// BSD `finitef(x)`.
pub fn finitef(x: f32) -> bool {
    crate::isfinitef(x)
}

/// BSD `gamma(x)`: the historical name of `lgamma` (it returns the log).
pub fn gamma(x: f64) -> f64 {
    crate::lgamma(x)
}

/// BSD `gammaf(x)`.
pub fn gammaf(x: f32) -> f32 {
    crate::lgammaf(x)
}

/// BSD `drem(x, y)`: the IEEE remainder, computed as `x - rint(x/y) * y`.
///
/// The quotient is rounded to nearest-even by [`crate::rint`]; for a quotient so large that the
/// division loses the integer part the caller should use `fmod` semantics instead.
pub fn drem(x: f64, y: f64) -> f64 {
    x - crate::rint(x / y) * y
}

/// BSD `dremf(x, y)`.
pub fn dremf(x: f32, y: f32) -> f32 {
    x - crate::rintf(x / y) * y
}

/// BSD `scalb(x, n)`: `x * 2^n`, the historical name of `scalbn`.
pub fn scalb(x: f64, n: f64) -> f64 {
    crate::scalbn(x, n as i32)
}

/// BSD `scalbf(x, n)`.
pub fn scalbf(x: f32, n: f32) -> f32 {
    crate::scalbnf(x, n as i32)
}

/// BSD `significand(x)`: `x` scaled to `[1, 2)`.
pub fn significand(x: f64) -> f64 {
    if x == 0.0 || !crate::isfinite(x) {
        return x;
    }
    crate::scalbn(x, -crate::ilogb(x))
}

/// BSD `significandf(x)`.
pub fn significandf(x: f32) -> f32 {
    if x == 0.0 || !crate::isfinitef(x) {
        return x;
    }
    crate::scalbnf(x, -crate::ilogbf(x))
}

#[cfg(test)]
#[path = "legacy_ut.rs"]
mod tests;

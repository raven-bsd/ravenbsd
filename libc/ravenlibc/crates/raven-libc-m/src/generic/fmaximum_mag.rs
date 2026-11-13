// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C23 `fmaximum_mag`: the argument with the greater magnitude.
//!
//! Behaviour read from ISO C23 7.12.12.5 and the FreeBSD `lib/msun` manual page (the donor's C
//! `fmaximum_mag` implementation is not in the pinned checkout): returns `x` if `|x| > |y|`,
//! `y` if `|y| > |x|`, and otherwise the value `fmaximum` would return (so equal magnitudes fall
//! back to the maximum, and `+0.0` wins over `-0.0`). A quiet NaN argument returns the other
//! argument; two NaNs return a NaN.

use crate::support::Float;

/// C23 `fmaximum_mag`.
#[inline]
pub fn fmaximum_mag<F: Float>(x: F, y: F) -> F {
    if x.is_nan() {
        return y.canonicalize();
    }
    if y.is_nan() {
        return x.canonicalize();
    }
    if x.abs() > y.abs() {
        return x.canonicalize();
    }
    if y.abs() > x.abs() {
        return y.canonicalize();
    }
    super::fmaximum(x, y)
}

/// C23 `fminimum_mag`: the argument with the lesser magnitude.
#[inline]
pub fn fminimum_mag<F: Float>(x: F, y: F) -> F {
    if x.is_nan() {
        return y.canonicalize();
    }
    if y.is_nan() {
        return x.canonicalize();
    }
    if x.abs() < y.abs() {
        return x.canonicalize();
    }
    if y.abs() < x.abs() {
        return y.canonicalize();
    }
    super::fminimum(x, y)
}

#[cfg(test)]
#[path = "fmaximum_mag_ut.rs"]
mod tests;

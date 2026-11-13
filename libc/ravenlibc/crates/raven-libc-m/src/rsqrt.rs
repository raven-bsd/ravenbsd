// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `rsqrt`: the reciprocal square root, `1 / sqrt(x)`.
//!
//! Functional specification read from the FreeBSD `lib/msun` `s_rsqrt.c`/`s_rsqrtf.c` comments and
//! behaviour (BSD-2-Clause, tier A); this implementation is written for RavenBSD from that
//! specification, not copied. Donor semantics:
//!
//! * `rsqrt(+-0) = +-inf` and raises divide-by-zero (the division by zero does that);
//! * `rsqrt(NaN) = NaN`;
//! * `rsqrt(+inf) = +0`, `rsqrt(-inf) = NaN` (the `sqrt` of a negative is invalid);
//! * `rsqrt(x < 0) = NaN` and raises invalid;
//! * otherwise the result is accurate to one ULP.
//!
//! Both `sqrt` and the division are correctly rounded, so the composition is within one ULP of
//! the exact result; the msun donor refines the last bit with a Goldschmidt step and a rounding
//! mode switch and reaches the same one-ULP bound (its comment reports 1.24 ULP over 5e8 inputs).

use crate::sqrt;

/// The reciprocal square root of `x`.
#[inline]
pub fn rsqrt(x: f64) -> f64 {
    1.0 / sqrt(x)
}

/// The reciprocal square root of an f32.
#[inline]
pub fn rsqrtf(x: f32) -> f32 {
    1.0 / crate::sqrtf(x)
}

#[cfg(test)]
#[path = "rsqrt_ut.rs"]
mod tests;

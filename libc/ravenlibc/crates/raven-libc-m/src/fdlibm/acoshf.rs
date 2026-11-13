// SPDX-License-Identifier: MIT
// Copyright (c) 2018 Jorge Aparicio
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from rust:library/compiler-builtins/libm/src/math/acoshf.rs @ 8bab26f4f68e0e26f0bb7960be334d5b520ea452
use super::{Float, log1pf, logf, sqrtf};

const LN2: f32 = 0.693147180559945309417232121458176568;

/// Inverse hyperbolic cosine (f32)
///
/// Calculates the inverse hyperbolic cosine of `x`.
/// Is defined as `log(x + sqrt(x*x-1))`.
/// `x` must be a number greater than or equal to 1.
#[cfg_attr(assert_no_panic, no_panic::no_panic)]
pub fn acoshf(x: f32) -> f32 {
    let ux = x.to_bits();

    /* x < 1 domain error is handled in the called functions */
    if (ux & !f32::SIGN_MASK) < 2_f32.to_bits() {
        /* |x| < 2, invalid if x < 1 */
        /* up to 2ulp error in [1,1.125] */
        let x_1 = x - 1.0;
        log1pf(x_1 + sqrtf(x_1 * x_1 + 2.0 * x_1))
    } else if ux < ((1 << 12) as f32).to_bits() {
        /* 2 <= x < 0x1p12 */
        logf(2.0 * x - 1.0 / (x + sqrtf(x * x - 1.0)))
    } else {
        /* x >= 0x1p12 or x <= -2 or nan */
        logf(x) + LN2
    }
}

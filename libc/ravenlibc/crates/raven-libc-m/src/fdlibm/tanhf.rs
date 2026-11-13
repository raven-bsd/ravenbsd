// SPDX-License-Identifier: MIT
// Copyright (c) 2018 Jorge Aparicio
// RavenBSD: transpiled from rust:library/compiler-builtins/libm/src/math/tanhf.rs @ 8bab26f4f68e0e26f0bb7960be334d5b520ea452
use super::expm1f;

/// The hyperbolic tangent of `x` (f32).
///
/// `x` is specified in radians.
#[cfg_attr(assert_no_panic, no_panic::no_panic)]
pub fn tanhf(mut x: f32) -> f32 {
    /* x = |x| */
    let mut ix = x.to_bits();
    let sign = (ix >> 31) != 0;
    ix &= 0x7fffffff;
    x = f32::from_bits(ix);
    let w = ix;

    let tt = if w > 0x3f0c9f54 {
        /* |x| > log(3)/2 ~= 0.5493 or nan */
        if w > 0x41200000 {
            /* |x| > 10 */
            1. + 0. / x
        } else {
            let t = expm1f(2. * x);
            1. - 2. / (t + 2.)
        }
    } else if w > 0x3e82c578 {
        /* |x| > log(5/3)/2 ~= 0.2554 */
        let t = expm1f(2. * x);
        t / (t + 2.)
    } else if w >= 0x00800000 {
        /* |x| >= 0x1p-126 */
        let t = expm1f(-2. * x);
        -t / (t + 2.)
    } else {
        /* |x| is subnormal */
        force_eval!(x * x);
        x
    };
    if sign { -tt } else { tt }
}

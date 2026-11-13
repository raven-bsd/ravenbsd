// SPDX-License-Identifier: MIT
// Copyright (c) 2018 Jorge Aparicio
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from rust:library/compiler-builtins/libm/src/math/ldexp.rs @ 8bab26f4f68e0e26f0bb7960be334d5b520ea452
#[cfg(f16_enabled)]
#[cfg_attr(assert_no_panic, no_panic::no_panic)]
pub fn ldexpf16(x: f16, n: i32) -> f16 {
    super::scalbnf16(x, n)
}

#[cfg_attr(assert_no_panic, no_panic::no_panic)]
pub fn ldexpf(x: f32, n: i32) -> f32 {
    crate::scalbnf(x, n)
}

#[cfg_attr(assert_no_panic, no_panic::no_panic)]
pub fn ldexp(x: f64, n: i32) -> f64 {
    crate::scalbn(x, n)
}

#[cfg(f128_enabled)]
#[cfg_attr(assert_no_panic, no_panic::no_panic)]
pub fn ldexpf128(x: f128, n: i32) -> f128 {
    super::scalbnf128(x, n)
}

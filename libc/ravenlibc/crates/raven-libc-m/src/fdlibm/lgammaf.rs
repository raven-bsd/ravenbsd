// SPDX-License-Identifier: MIT
// Copyright (c) 2018 Jorge Aparicio
// RavenBSD: transpiled from rust:library/compiler-builtins/libm/src/math/lgammaf.rs @ 8bab26f4f68e0e26f0bb7960be334d5b520ea452
use super::lgammaf_r;

/// The natural logarithm of the
/// [Gamma function](https://en.wikipedia.org/wiki/Gamma_function) (f32).
#[cfg_attr(assert_no_panic, no_panic::no_panic)]
pub fn lgammaf(x: f32) -> f32 {
    lgammaf_r(x).0
}

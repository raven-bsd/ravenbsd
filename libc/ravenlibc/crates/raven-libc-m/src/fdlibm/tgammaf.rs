// SPDX-License-Identifier: MIT
// Copyright (c) 2018 Jorge Aparicio
// RavenBSD: transpiled from rust:library/compiler-builtins/libm/src/math/tgammaf.rs @ 8bab26f4f68e0e26f0bb7960be334d5b520ea452
use super::tgamma;

/// The [Gamma function](https://en.wikipedia.org/wiki/Gamma_function) (f32).
#[cfg_attr(assert_no_panic, no_panic::no_panic)]
pub fn tgammaf(x: f32) -> f32 {
    tgamma(x as f64) as f32
}

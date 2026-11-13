// SPDX-License-Identifier: MIT
// Copyright (c) 2018 Jorge Aparicio
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from rust:library/compiler-builtins/libm/src/math/generic/fdim.rs @ 8bab26f4f68e0e26f0bb7960be334d5b520ea452
use crate::support::Float;

#[inline]
pub fn fdim<F: Float>(x: F, y: F) -> F {
    if x <= y { F::ZERO } else { x - y }
}

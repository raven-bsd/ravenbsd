// SPDX-License-Identifier: MIT
// Copyright (c) 2018 Jorge Aparicio
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from rust:library/compiler-builtins/libm/src/math/generic/fabs.rs @ 8bab26f4f68e0e26f0bb7960be334d5b520ea452
use crate::support::Float;

/// Absolute value.
#[inline]
pub fn fabs<F: Float>(x: F) -> F {
    let abs_mask = !F::SIGN_MASK;
    F::from_bits(x.to_bits() & abs_mask)
}

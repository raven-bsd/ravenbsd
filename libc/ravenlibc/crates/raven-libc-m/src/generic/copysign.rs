// SPDX-License-Identifier: MIT
// Copyright (c) 2018 Jorge Aparicio
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from rust:library/compiler-builtins/libm/src/math/generic/copysign.rs @ 8bab26f4f68e0e26f0bb7960be334d5b520ea452
use crate::support::Float;

/// Copy the sign of `y` to `x`.
#[inline]
pub fn copysign<F: Float>(x: F, y: F) -> F {
    let mut ux = x.to_bits();
    let uy = y.to_bits();
    ux &= !F::SIGN_MASK;
    ux |= uy & F::SIGN_MASK;
    F::from_bits(ux)
}

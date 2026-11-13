// SPDX-License-Identifier: MIT
// Copyright (c) 2018 Jorge Aparicio
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from rust:library/compiler-builtins/libm/src/math/generic/round.rs @ 8bab26f4f68e0e26f0bb7960be334d5b520ea452
use super::{copysign, trunc_status};
use crate::support::{Float, MinInt};

#[inline]
pub fn round<F: Float>(x: F) -> F {
    let f0p5 = F::from_parts(false, F::EXP_BIAS - 1, F::Int::ZERO); // 0.5
    let f0p25 = F::from_parts(false, F::EXP_BIAS - 2, F::Int::ZERO); // 0.25

    trunc_status(x + copysign(f0p5 - f0p25 * F::EPSILON, x)).val
}

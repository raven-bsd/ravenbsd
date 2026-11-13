// SPDX-License-Identifier: MIT
// Copyright (c) 2018 Jorge Aparicio
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from rust:library/compiler-builtins/libm/src/math/generic/frexp.rs @ 8bab26f4f68e0e26f0bb7960be334d5b520ea452
use crate::support::{CastFrom, Float};

#[inline]
pub fn frexp<F: Float>(x: F) -> (F, i32) {
    let mut ix = x.to_bits();
    let mut ee = x.ex() as i32;

    if ee == 0 {
        if x == F::ZERO {
            return (x, 0);
        }

        // Subnormals, needs to be normalized first
        ix &= F::SIG_MASK;
        (ee, ix) = F::normalize(ix);
        ix |= x.to_bits() & F::SIGN_MASK;
    } else if ee == F::EXP_SAT as i32 {
        // inf or  NaN
        return (x, 0);
    }

    let e = ee - (F::EXP_BIAS as i32 - 1);
    ix &= F::SIGN_MASK | F::SIG_MASK;
    ix |= F::Int::cast_from(F::EXP_BIAS - 1) << F::SIG_BITS;
    (F::from_bits(ix), e)
}

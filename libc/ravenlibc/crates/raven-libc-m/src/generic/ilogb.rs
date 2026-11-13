// SPDX-License-Identifier: MIT
// Copyright (c) 2018 Jorge Aparicio
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from rust:library/compiler-builtins/libm/src/math/generic/ilogb.rs @ 8bab26f4f68e0e26f0bb7960be334d5b520ea452
use crate::consts::{FP_ILOGB0, FP_ILOGBNAN};
use crate::support::{Float, MinInt};

#[inline]
pub fn ilogb<F: Float>(x: F) -> i32 {
    let zero = F::Int::ZERO;
    let mut i = x.to_bits();
    let e = x.ex() as i32;

    if e == 0 {
        i <<= F::EXP_BITS + 1;
        if i == F::Int::ZERO {
            force_eval!(0.0 / 0.0);
            return FP_ILOGB0;
        }
        /* subnormal x */
        let mut e = -(F::EXP_BIAS as i32);
        while i >> (F::BITS - 1) == zero {
            e -= 1;
            i <<= 1;
        }
        e
    } else if e == F::EXP_SAT as i32 {
        force_eval!(0.0 / 0.0);
        if i << (F::EXP_BITS + 1) != zero {
            FP_ILOGBNAN
        } else {
            i32::MAX
        }
    } else {
        e - F::EXP_BIAS as i32
    }
}

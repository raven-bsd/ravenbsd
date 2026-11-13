// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The single place that raises floating-point exceptions.
//!
//! On a hard-float target the flags come from real hardware operations on `black_box` operands
//! (so the optimiser cannot fold them): `0/0` (invalid), `1/0` (divide-by-zero), an overflowing
//! product, a subnormal-producing division and a repeating quotient. On the soft-float profile
//! the flags are ORed into the software word (A-num-4, 4.8).

use crate::consts::{FE_DIVBYZERO, FE_INEXACT, FE_INVALID, FE_OVERFLOW, FE_UNDERFLOW};

/// Raise the exceptions named by `flags`.
pub fn raise(flags: u32) {
    if crate::SOFT_FLOAT {
        crate::fenv::raise_software(flags);
        return;
    }
    use core::hint::black_box;
    if flags & FE_INVALID != 0 {
        let inf = black_box(f64::INFINITY);
        let _ = black_box(inf - inf);
    }
    if flags & FE_DIVBYZERO != 0 {
        let one = black_box(1.0f64);
        let zero = black_box(0.0f64);
        let _ = black_box(one / zero);
    }
    if flags & FE_OVERFLOW != 0 {
        let huge = black_box(f64::MAX);
        let _ = black_box(huge * huge);
    }
    if flags & FE_UNDERFLOW != 0 {
        let tiny = black_box(f64::from_bits(3));
        let two = black_box(2.0f64);
        let _ = black_box(tiny / two);
    }
    if flags & FE_INEXACT != 0 {
        let one = black_box(1.0f64);
        let three = black_box(3.0f64);
        let _ = black_box(one / three);
    }
}

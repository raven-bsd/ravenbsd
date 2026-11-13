// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `FLT_ROUNDS`: the C `fegetround` numbering.

use crate::consts::{FE_DOWNWARD, FE_TONEAREST, FE_TOWARDZERO, FE_UPWARD};

/// `FLT_ROUNDS`: 0 toward zero, 1 nearest, 2 up, 3 down (the C `float.h` order).
pub fn flt_rounds() -> i32 {
    let round = crate::fenv::fegetround() as u32;
    match round {
        FE_TONEAREST => 1,
        FE_UPWARD => 2,
        FE_DOWNWARD => 3,
        FE_TOWARDZERO => 0,
        _ => -1,
    }
}

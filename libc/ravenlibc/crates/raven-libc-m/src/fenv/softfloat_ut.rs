// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:lib/msun/src/fenv-softfloat.h @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// (tests for the software fenv backend, run with --features soft-float)
#![allow(clippy::panic, clippy::unwrap_used)]

use super::*;
use crate::consts::{FE_ALL_EXCEPT, FE_INEXACT, FE_OVERFLOW, FE_TONEAREST, FE_UPWARD};

#[test]
fn flags_are_software_state() {
    feclearexcept(FE_ALL_EXCEPT as i32);
    assert_eq!(fetestexcept(FE_ALL_EXCEPT as i32), 0);
    feraiseexcept((FE_INEXACT | FE_OVERFLOW) as i32);
    assert_eq!(fetestexcept(FE_INEXACT as i32), FE_INEXACT as i32);
    assert_eq!(fetestexcept(FE_OVERFLOW as i32), FE_OVERFLOW as i32);
    feclearexcept(FE_INEXACT as i32);
    assert_eq!(fetestexcept(FE_INEXACT as i32), 0);
    assert_eq!(fetestexcept(FE_OVERFLOW as i32), FE_OVERFLOW as i32);
    feclearexcept(FE_ALL_EXCEPT as i32);
}

#[test]
fn the_rounding_mode_is_nearest_only() {
    assert_eq!(fegetround(), FE_TONEAREST as i32);
    assert_eq!(fesetround(FE_TONEAREST as i32), 0);
    assert_ne!(fesetround(FE_UPWARD as i32), 0);
    assert_eq!(fegetround(), FE_TONEAREST as i32);
}

#[test]
fn environments_round_trip() {
    feclearexcept(FE_ALL_EXCEPT as i32);
    feraiseexcept(FE_OVERFLOW as i32);
    let mut saved = FenvT::default();
    assert_eq!(fegetenv(&mut saved), 0);
    feclearexcept(FE_ALL_EXCEPT as i32);
    assert_eq!(fesetenv(&saved), 0);
    assert_eq!(fetestexcept(FE_OVERFLOW as i32), FE_OVERFLOW as i32);
    feclearexcept(FE_ALL_EXCEPT as i32);
}

#[test]
fn hold_and_update_merge_the_new_flags() {
    feclearexcept(FE_ALL_EXCEPT as i32);
    feraiseexcept(FE_INEXACT as i32);
    let mut saved = FenvT::default();
    assert_eq!(feholdexcept(&mut saved), 0);
    assert_eq!(fetestexcept(FE_ALL_EXCEPT as i32), 0);
    feraiseexcept(FE_OVERFLOW as i32);
    assert_eq!(feupdateenv(&saved), 0);
    assert_eq!(fetestexcept(FE_INEXACT as i32), FE_INEXACT as i32);
    assert_eq!(fetestexcept(FE_OVERFLOW as i32), FE_OVERFLOW as i32);
    feclearexcept(FE_ALL_EXCEPT as i32);
}

#[test]
fn exception_masks_are_software_too() {
    let previous = fedisableexcept(FE_ALL_EXCEPT as i32);
    assert_eq!(fegetexcept(), 0);
    assert_eq!(feenableexcept(FE_INEXACT as i32), 0);
    assert_eq!(fegetexcept(), FE_INEXACT as i32);
    assert_eq!(fedisableexcept(FE_INEXACT as i32), FE_INEXACT as i32);
    let _ = feenableexcept(previous);
    let _ = fedisableexcept(FE_ALL_EXCEPT as i32);
}

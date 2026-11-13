// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:lib/msun/amd64/fenv.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// (tests for the x86_64 fenv backend, written from the donor semantics)
#![allow(clippy::panic, clippy::unwrap_used)]

//! x86_64 `fenv` tests: rounds, flags, saved environments and trap masks.

use super::*;
use crate::consts::{
    FE_DIVBYZERO, FE_DOWNWARD, FE_INEXACT, FE_INVALID, FE_OVERFLOW, FE_TONEAREST, FE_TOWARDZERO,
    FE_UNDERFLOW, FE_UPWARD,
};

#[test]
fn the_record_layouts_are_the_abi_ones() {
    assert_eq!(core::mem::size_of::<FenvT>(), 32);
    assert_eq!(core::mem::size_of::<FexceptT>(), 4);
    assert_eq!(core::mem::size_of::<Femode>(), 8);
    assert_eq!(core::mem::align_of::<FenvT>(), 16);
}

#[test]
fn every_rounding_mode_round_trips() {
    let saved = fegetround();
    for mode in [FE_TONEAREST, FE_DOWNWARD, FE_UPWARD, FE_TOWARDZERO] {
        assert_eq!(fesetround(mode as i32), 0);
        assert_eq!(fegetround(), mode as i32);
    }
    assert_eq!(fesetround(0x1234), -1);
    assert_eq!(fesetround(saved), 0);
}

#[test]
fn flags_can_be_cleared_raised_and_tested() {
    feclearexcept(FE_ALL_EXCEPT as i32);
    assert_eq!(fetestexcept(FE_ALL_EXCEPT as i32), 0);
    for flag in [
        FE_INVALID,
        FE_DIVBYZERO,
        FE_OVERFLOW,
        FE_UNDERFLOW,
        FE_INEXACT,
    ] {
        feclearexcept(FE_ALL_EXCEPT as i32);
        assert_eq!(feraiseexcept(flag as i32), 0);
        assert_ne!(fetestexcept(flag as i32), 0, "flag {flag:#x}");
        feclearexcept(flag as i32);
        assert_eq!(fetestexcept(flag as i32), 0);
    }
    feclearexcept(FE_ALL_EXCEPT as i32);
}

#[test]
fn exception_flags_round_trip_through_the_saved_word() {
    feclearexcept(FE_ALL_EXCEPT as i32);
    feraiseexcept((FE_INEXACT | FE_OVERFLOW) as i32);
    let mut flags = FexceptT::default();
    assert_eq!(fegetexceptflag(&mut flags, FE_ALL_EXCEPT as i32), 0);
    assert_eq!(
        flags.0 & (FE_INEXACT | FE_OVERFLOW),
        FE_INEXACT | FE_OVERFLOW
    );
    feclearexcept(FE_ALL_EXCEPT as i32);
    assert_eq!(
        fesetexceptflag(&flags, (FE_INEXACT | FE_OVERFLOW) as i32),
        0
    );
    assert_ne!(fetestexcept(FE_INEXACT as i32), 0);
    assert_ne!(fetestexcept(FE_OVERFLOW as i32), 0);
    feclearexcept(FE_ALL_EXCEPT as i32);
}

#[test]
fn the_environment_saves_and_restores_the_mode() {
    let saved = fegetround();
    assert_eq!(fesetround(FE_DOWNWARD as i32), 0);
    let mut env = FenvT {
        x87: [0u8; 28],
        mxcsr: 0,
    };
    assert_eq!(fegetenv(&mut env), 0);
    assert_eq!(fesetround(FE_UPWARD as i32), 0);
    assert_eq!(fesetenv(&env), 0);
    assert_eq!(fegetround(), FE_DOWNWARD as i32);
    assert_eq!(fesetround(saved), 0);
}

#[test]
fn hold_then_update_keeps_the_new_flags() {
    feclearexcept(FE_ALL_EXCEPT as i32);
    feraiseexcept(FE_INEXACT as i32);
    let mut env = FenvT {
        x87: [0u8; 28],
        mxcsr: 0,
    };
    assert_eq!(feholdexcept(&mut env), 0);
    assert_eq!(
        fetestexcept(FE_ALL_EXCEPT as i32),
        0,
        "hold clears the flags"
    );
    feraiseexcept(FE_OVERFLOW as i32);
    assert_eq!(feupdateenv(&env), 0);
    assert_ne!(fetestexcept(FE_INEXACT as i32), 0);
    assert_ne!(fetestexcept(FE_OVERFLOW as i32), 0);
    feclearexcept(FE_ALL_EXCEPT as i32);
}

#[test]
fn trap_masks_can_be_enabled_and_disabled() {
    let previous = fedisableexcept(FE_ALL_EXCEPT as i32);
    assert_eq!(fegetexcept(), 0);
    let old = feenableexcept(FE_INEXACT as i32);
    assert_eq!(old, 0);
    assert_eq!(fegetexcept(), FE_INEXACT as i32);
    assert_eq!(fedisableexcept(FE_INEXACT as i32), FE_INEXACT as i32);
    assert_eq!(fegetexcept(), 0);
    let _ = feenableexcept(previous & FE_ALL_EXCEPT as i32);
    let _ = fedisableexcept(FE_ALL_EXCEPT as i32);
}

#[test]
fn the_mode_record_saves_the_control_state() {
    let saved = fegetround();
    assert_eq!(fesetround(FE_TOWARDZERO as i32), 0);
    let mut mode = Femode::default();
    assert_eq!(fegetmode(&mut mode), 0);
    assert_eq!(fesetround(FE_UPWARD as i32), 0);
    assert_eq!(fesetmode(&mode), 0);
    assert_eq!(fegetround(), FE_TOWARDZERO as i32);
    assert_eq!(fesetround(saved), 0);
}

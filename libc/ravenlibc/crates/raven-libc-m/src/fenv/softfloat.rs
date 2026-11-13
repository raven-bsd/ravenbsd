// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2004-2005 David Schultz <das@FreeBSD.ORG>
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:lib/msun/src/fenv-softfloat.h @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// (file revision tag: none)
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE AUTHOR AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.
//
//! Software floating-point environment for the soft-float profile (4.8).
//!
//! One atomic word holds the exception flags, the trap mask and the rounding mode: the only
//! static in the crate. The rounding mode is always nearest on this profile (ISO C 7.6.3.2
//! permits `fesetround` to fail for other modes).

use core::sync::atomic::{AtomicU32, Ordering};

use crate::consts::{FE_ALL_EXCEPT, FE_TONEAREST};

const FLAGS_SHIFT: u32 = 0;
const MASK_SHIFT: u32 = 8;
const ROUND_SHIFT: u32 = 16;
const ROUND_MASK: u32 = 0x3;

static STATE: AtomicU32 = AtomicU32::new(0);

fn load() -> u32 {
    STATE.load(Ordering::Relaxed)
}

fn flags_of(word: u32) -> u32 {
    (word >> FLAGS_SHIFT) & FE_ALL_EXCEPT
}

fn mask_of(word: u32) -> u32 {
    (word >> MASK_SHIFT) & FE_ALL_EXCEPT
}

fn round_of(word: u32) -> u32 {
    (word >> ROUND_SHIFT) & ROUND_MASK
}

fn store(flags: u32, mask: u32, round: u32) -> u32 {
    let word = (flags & FE_ALL_EXCEPT) << FLAGS_SHIFT
        | (mask & FE_ALL_EXCEPT) << MASK_SHIFT
        | (round & ROUND_MASK) << ROUND_SHIFT;
    STATE.store(word, Ordering::Relaxed);
    word
}

/// Raise exception flags from the software arithmetic (the `__softfloat_float_raise` hook).
pub(crate) fn raise(flags: u32) {
    let word = load();
    store(flags_of(word) | flags, mask_of(word), round_of(word));
}

/// The C `fenv_t`: flags, mask and rounding mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FenvT {
    word: u32,
}

///`fexcept_t`: the exception flags.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FexceptT(pub u32);

/// `femode_t`: the mask and rounding mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Femode {
    word: u32,
}

/// The default environment.
pub const FE_DFL_ENV: FenvT = FenvT { word: 0 };

/// Clear the given flags.
pub fn feclearexcept(excepts: i32) -> i32 {
    let word = load();
    store(
        flags_of(word) & !(excepts as u32),
        mask_of(word),
        round_of(word),
    );
    0
}

/// Read the given flags.
pub fn fegetexceptflag(flagp: &mut FexceptT, excepts: i32) -> i32 {
    flagp.0 = flags_of(load()) & (excepts as u32) & FE_ALL_EXCEPT;
    0
}

/// Replace the given flags.
pub fn fesetexceptflag(flagp: &FexceptT, excepts: i32) -> i32 {
    let word = load();
    let mask = (excepts as u32) & FE_ALL_EXCEPT;
    store(
        (flags_of(word) & !mask) | (flagp.0 & mask),
        mask_of(word),
        round_of(word),
    );
    0
}

/// Raise the given flags.
pub fn feraiseexcept(excepts: i32) -> i32 {
    raise((excepts as u32) & FE_ALL_EXCEPT);
    0
}

/// Test the given flags.
pub fn fetestexcept(excepts: i32) -> i32 {
    (flags_of(load()) & (excepts as u32) & FE_ALL_EXCEPT) as i32
}

/// The rounding mode: always nearest on the soft profile.
pub fn fegetround() -> i32 {
    FE_TONEAREST as i32
}

/// Set the rounding mode: only `FE_TONEAREST` is supported.
pub fn fesetround(round: i32) -> i32 {
    if round as u32 == FE_TONEAREST {
        let word = load();
        store(flags_of(word), mask_of(word), 0);
        0
    } else {
        1
    }
}

/// Save the environment.
pub fn fegetenv(envp: &mut FenvT) -> i32 {
    envp.word = load();
    0
}

/// Save the environment and clear the flags (non-stop mode).
pub fn feholdexcept(envp: &mut FenvT) -> i32 {
    let word = load();
    envp.word = word;
    store(0, mask_of(word), round_of(word));
    0
}

/// Restore the environment.
pub fn fesetenv(envp: &FenvT) -> i32 {
    STATE.store(envp.word, Ordering::Relaxed);
    0
}

/// Restore the default environment.
pub fn fesetenv_default() -> i32 {
    fesetenv(&FE_DFL_ENV)
}

/// Restore the environment, keeping the flags raised since it was saved.
pub fn feupdateenv(envp: &FenvT) -> i32 {
    let raised = flags_of(load());
    fesetenv(envp);
    raise(raised);
    0
}

/// Enable the given trap mask; returns the previous mask.
pub fn feenableexcept(mask: i32) -> i32 {
    let word = load();
    let previous = mask_of(word);
    store(flags_of(word), previous | (mask as u32), round_of(word));
    previous as i32
}

/// Disable the given trap mask; returns the previous mask.
pub fn fedisableexcept(mask: i32) -> i32 {
    let word = load();
    let previous = mask_of(word);
    store(flags_of(word), previous & !(mask as u32), round_of(word));
    previous as i32
}

/// The currently enabled trap mask.
pub fn fegetexcept() -> i32 {
    mask_of(load()) as i32
}

/// Save the control state (mask and mode).
pub fn fegetmode(modep: &mut Femode) -> i32 {
    modep.word = load();
    0
}

/// Restore the control state.
pub fn fesetmode(modep: &Femode) -> i32 {
    let word = load();
    store(flags_of(word), mask_of(modep.word), round_of(modep.word));
    0
}

#[cfg(test)]
#[path = "softfloat_ut.rs"]
mod tests;

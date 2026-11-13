// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1983, 1993 The Regents of the University of California.
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from netbsd:common/lib/libc/stdlib/random.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: random.c,v 1.9 2026/10/11 20:08:24 tls Exp $)
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
// 3. Neither the name of the University nor the names of its contributors
//    may be used to endorse or promote products derived from this software
//    without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.

#![allow(clippy::arithmetic_side_effects)] // RavenBSD: donor state arithmetic (wrapping)

//! The five-type additive-feedback `random` generator.
//!
//! Deviation D-1 (PROVENANCE `notes`): `srandom(0)` behaves as `srandom(1)`, as glibc does and
//! as the CVE-2012-1577 test requires. The donor's `1103515245` seeding branch is deleted (the
//! `USE_BETTER_RANDOM` Park-Miller branch is the only one), and its mutex macros are dropped
//! because the shell locks.

/// The `TYPE_0` linear congruential generator.
pub const TYPE_0: u32 = 0;
/// Polynomial `x^7 + x^3 + 1` (degree 7, separation 3).
pub const TYPE_1: u32 = 1;
/// Polynomial `x^15 + x + 1` (degree 15, separation 1).
pub const TYPE_2: u32 = 2;
/// Polynomial `x^31 + x^3 + 1` (degree 31, separation 3), the default.
pub const TYPE_3: u32 = 3;
/// Polynomial `x^63 + x + 1` (degree 63, separation 1).
pub const TYPE_4: u32 = 4;
/// The number of state types.
pub const MAX_TYPES: u32 = 5;

const DEGREES: [usize; 5] = [0, 7, 15, 31, 63];
const SEPS: [usize; 5] = [0, 3, 1, 3, 1];

/// The per-generator control block; indices point into the state slice after its type word.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct RandomCtl {
    /// One of [`TYPE_0`]..[`TYPE_4`].
    pub r#type: u32,
    /// Polynomial degree.
    pub deg: usize,
    /// Separation between the front and rear pointers.
    pub sep: usize,
    /// Front pointer, an index into the generator state (word 0 of the buffer excluded).
    pub fptr: usize,
    /// Rear pointer.
    pub rptr: usize,
}

impl RandomCtl {
    /// The default control block: `TYPE_3`, degree 31, separation 3.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            r#type: TYPE_3,
            deg: 31,
            sep: 3,
            fptr: 3,
            rptr: 0,
        }
    }

    /// The number of state words (type word included) a buffer of this type needs.
    #[must_use]
    pub const fn words(&self) -> usize {
        if self.r#type == TYPE_0 {
            2
        } else {
            1 + self.deg
        }
    }
}

impl Default for RandomCtl {
    fn default() -> Self {
        Self::new()
    }
}

/// A refusal from `initstate`/`setstate`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RandErr {
    /// `nbytes < 8`, an unknown type word, or a state buffer that is too small.
    Invalid,
}

/// Park-Miller seeding of the state words (the donor's `USE_BETTER_RANDOM` branch).
fn park_miller(state: &mut [u32], seed: u32) {
    let Some(first) = state.get_mut(0) else {
        return;
    };
    *first = seed;
    let mut index = 1usize;
    while index < state.len() {
        let x1 = match state.get(index.wrapping_sub(1)) {
            Some(value) => *value as i32,
            None => 1,
        };
        let hi = x1 / 127_773;
        let lo = x1 % 127_773;
        let mut t = 16_807i32
            .wrapping_mul(lo)
            .wrapping_sub(2_836i32.wrapping_mul(hi));
        if t <= 0 {
            t = t.wrapping_add(0x7fff_ffff);
        }
        if let Some(slot) = state.get_mut(index) {
            *slot = t as u32;
        }
        index = index.wrapping_add(1);
    }
}

/// `srandom(seed)` over the caller's state buffer.
pub fn srandom(ctl: &mut RandomCtl, st: &mut [u32], seed: u32) {
    // RavenBSD: D-1, srandom(0) behaves as srandom(1) (glibc, CVE-2012-1577 test).
    let seed = if seed == 0 { 1 } else { seed };
    if ctl.r#type == TYPE_0 {
        if let Some(slot) = st.get_mut(1) {
            *slot = seed;
        }
        return;
    }
    let deg = ctl.deg;
    if st.len() < deg + 1 {
        return;
    }
    if let Some(state) = st.get_mut(1..=deg) {
        park_miller(state, seed);
    }
    ctl.fptr = ctl.sep;
    ctl.rptr = 0;
    let mut discarded = 0usize;
    while discarded < 10usize.wrapping_mul(deg) {
        let _ = random(ctl, st);
        discarded = discarded.wrapping_add(1);
    }
}

/// One `random()` value in `0..=RAND_MAX`.
pub fn random(ctl: &mut RandomCtl, st: &mut [u32]) -> u32 {
    if ctl.r#type == TYPE_0 {
        let value = st.get(1).copied().unwrap_or(0);
        let next = value.wrapping_mul(1_103_515_245).wrapping_add(12_345) & 0x7fff_ffff;
        if let Some(slot) = st.get_mut(1) {
            *slot = next;
        }
        return next;
    }
    let front = ctl.fptr.wrapping_add(1);
    let rear = ctl.rptr.wrapping_add(1);
    let Some(rear_value) = st.get(rear).copied() else {
        return 0;
    };
    let Some(front_value) = st.get_mut(front) else {
        return 0;
    };
    *front_value = front_value.wrapping_add(rear_value);
    let result = (*front_value >> 1) & 0x7fff_ffff;
    ctl.fptr = ctl.fptr.wrapping_add(1);
    if ctl.fptr >= ctl.deg {
        ctl.fptr = 0;
        ctl.rptr = ctl.rptr.wrapping_add(1);
    } else {
        ctl.rptr = ctl.rptr.wrapping_add(1);
    }
    if ctl.rptr >= ctl.deg {
        ctl.rptr = 0;
    }
    result
}

/// `initstate(seed, st, nbytes)`: choose the type from the byte count and seed.
///
/// # Errors
///
/// [`RandErr::Invalid`] when `nbytes < 8` or the caller's state buffer is too small.
pub fn initstate(
    ctl: &mut RandomCtl,
    st: &mut [u32],
    seed: u32,
    nbytes: usize,
) -> Result<(), RandErr> {
    if nbytes < 8 {
        return Err(RandErr::Invalid);
    }
    let r#type = if nbytes < 32 {
        TYPE_0
    } else if nbytes < 64 {
        TYPE_1
    } else if nbytes < 128 {
        TYPE_2
    } else if nbytes < 256 {
        TYPE_3
    } else {
        TYPE_4
    };
    let index = r#type as usize;
    let deg = DEGREES.get(index).copied().unwrap_or(0);
    let sep = SEPS.get(index).copied().unwrap_or(0);
    let needed = if r#type == TYPE_0 { 2 } else { deg + 1 };
    if st.len() < needed {
        return Err(RandErr::Invalid);
    }
    ctl.r#type = r#type;
    ctl.deg = deg;
    ctl.sep = sep;
    srandom(ctl, st, seed);
    if let Some(slot) = st.get_mut(0) {
        *slot = r#type;
    }
    Ok(())
}

/// `setstate(st)`: restore a state buffer produced by `initstate`/`setstate`.
///
/// # Errors
///
/// [`RandErr::Invalid`] when the type word names a type outside `0..=4` or the buffer is short.
pub fn setstate(ctl: &mut RandomCtl, st: &mut [u32]) -> Result<(), RandErr> {
    let word = st.first().copied().unwrap_or(0);
    let r#type = word % MAX_TYPES;
    let rear = (word / MAX_TYPES) as usize;
    let index = r#type as usize;
    let deg = DEGREES.get(index).copied().ok_or(RandErr::Invalid)?;
    let sep = SEPS.get(index).copied().ok_or(RandErr::Invalid)?;
    let needed = if r#type == TYPE_0 { 2 } else { deg + 1 };
    if st.len() < needed {
        return Err(RandErr::Invalid);
    }
    ctl.r#type = r#type;
    ctl.deg = deg;
    ctl.sep = sep;
    if r#type != TYPE_0 {
        ctl.rptr = rear % deg;
        ctl.fptr = (ctl.rptr + sep) % deg;
    }
    Ok(())
}

#[cfg(test)]
#[path = "random_ut.rs"]
mod tests;

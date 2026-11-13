// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)] // RavenBSD: const Park-Miller seeding

//!
//! `rand`/`srand`/`RAND_MAX`.
//!
//! CLEANROOM: the OpenBSD `rand.c` returns `arc4random() & RAND_MAX` unless
//! `srand_deterministic()` ran (A-stdlib-3), which breaks every ported program that seeds for a
//! reproducible sequence. This implementation is `random()`/`srandom()` over a separate
//! [`DefaultRandom`] (`TYPE_3`, default seed 1), the behaviour FreeBSD's `rand.c` describes and
//! glibc's sequence when seeded with 1.

use super::random::{RandomCtl, TYPE_3, random, srandom};

/// `RAND_MAX` on every Raven target (T-8).
pub const RAND_MAX: u32 = 0x7fff_ffff;

/// The process-default generator: `TYPE_3` state seeded with 1.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct DefaultRandom {
    /// The control block.
    pub ctl: RandomCtl,
    /// Word 0 is the type word, words 1..=31 the generator state.
    pub state: [u32; 32],
}

const fn seeded_state() -> [u32; 32] {
    let mut state = [0u32; 32];
    state[0] = TYPE_3;
    state[1] = 1;
    let mut index = 2usize;
    while index < 32 {
        let x1 = state[index - 1] as i32;
        let hi = x1 / 127_773;
        let lo = x1 % 127_773;
        let mut t = 16_807i32
            .wrapping_mul(lo)
            .wrapping_sub(2_836i32.wrapping_mul(hi));
        if t <= 0 {
            t = t.wrapping_add(0x7fff_ffff);
        }
        state[index] = t as u32;
        index += 1;
    }
    let mut fptr = 3usize;
    let mut rptr = 0usize;
    let mut discarded = 0usize;
    while discarded < 310 {
        let front = fptr + 1;
        let rear = rptr + 1;
        state[front] = state[front].wrapping_add(state[rear]);
        fptr += 1;
        if fptr >= 31 {
            fptr = 0;
            rptr += 1;
        } else {
            rptr += 1;
        }
        if rptr >= 31 {
            rptr = 0;
        }
        discarded += 1;
    }
    state
}

impl DefaultRandom {
    /// The default generator, exactly as if `initstate(1, state, 128)` had run.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            ctl: RandomCtl {
                r#type: TYPE_3,
                deg: 31,
                sep: 3,
                fptr: 3,
                rptr: 0,
            },
            state: seeded_state(),
        }
    }

    /// `rand()`.
    pub fn rand(&mut self) -> u32 {
        random(&mut self.ctl, &mut self.state)
    }

    /// `srand(seed)`.
    pub fn srand(&mut self, seed: u32) {
        srandom(&mut self.ctl, &mut self.state, seed);
    }
}

impl Default for DefaultRandom {
    fn default() -> Self {
        Self::new()
    }
}

/// `rand()` on the caller's default generator.
pub fn rand(generator: &mut DefaultRandom) -> u32 {
    generator.rand()
}

/// `srand(seed)` on the caller's default generator.
pub fn srand(generator: &mut DefaultRandom, seed: u32) {
    generator.srand(seed);
}

#[cfg(test)]
#[path = "rand_ut.rs"]
mod tests;

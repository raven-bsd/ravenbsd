// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic helpers for the S64 USB HID fuzz harness.
//!
//! The harness is host-only and uses a fixed linear congruential generator, so a failing case is
//! reproducible without a fuzz corpus download. The corpus and mutation tests live in
//! `tests/corpus.rs`.

#![forbid(unsafe_code)]

/// A deterministic 64-bit linear congruential generator (the PCG multiplier/step constants).
pub struct Lcg(u64);

impl Lcg {
    /// Seed the generator. A fixed seed makes every run identical.
    pub const fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// Next 32-bit value.
    pub fn next_u32(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as u32
    }

    /// Fill a byte slice.
    pub fn fill(&mut self, buffer: &mut [u8]) {
        for slot in buffer.iter_mut() {
            *slot = self.next_u32() as u8;
        }
    }

    /// A number in `0..bound` (zero when `bound == 0`).
    pub fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            return 0;
        }
        (self.next_u32() as usize) % bound
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

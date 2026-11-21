// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Small deterministic xorshift generator for the model tests.

/// A deterministic 64-bit xorshift generator.
pub struct Rng(u64);

impl Rng {
    /// Create a generator; zero is replaced by a fixed non-zero seed.
    pub const fn new(seed: u64) -> Self {
        Self(if seed == 0 {
            0x9e37_79b9_7f4a_7c15
        } else {
            seed
        })
    }

    /// Next 32-bit value.
    pub fn next_u32(&mut self) -> u32 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        (value >> 16) as u32
    }

    /// Uniform value in `0..bound` (bound must be non-zero).
    pub fn below(&mut self, bound: u32) -> u32 {
        self.next_u32() % bound.max(1)
    }

    /// Uniform value in `low..=high` (inclusive; `high >= low`).
    pub fn range(&mut self, low: u32, high: u32) -> u32 {
        if high <= low {
            return low;
        }
        low + self.below(high - low + 1)
    }
}

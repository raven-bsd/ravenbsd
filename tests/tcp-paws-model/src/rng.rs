// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic xorshift generator for the randomized timestamp sequences.

/// A deterministic 64-bit xorshift generator.
pub struct Rng(u64);

impl Rng {
    /// Create a generator; zero is replaced by a fixed non-zero seed.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self(if seed == 0 {
            0x9e37_79b9_7f4a_7c15
        } else {
            seed
        })
    }

    /// Next 64-bit value.
    pub fn next_u64(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }

    /// Next 32-bit value.
    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// Uniform value in `0..bound` (`bound` must be non-zero).
    pub fn below(&mut self, bound: u32) -> u32 {
        self.next_u32() % bound.max(1)
    }
}

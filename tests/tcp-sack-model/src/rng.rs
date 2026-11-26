// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic xorshift64* generator for the 10k-operation model.

/// A small deterministic PRNG.
#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// Seed the generator; zero is replaced with a fixed non-zero state.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9e37_79b9_7f4a_7c15
            } else {
                seed
            },
        }
    }

    /// Next 64-bit value.
    pub fn next_u64(&mut self) -> u64 {
        let mut value = self.state;
        value ^= value >> 12;
        value ^= value << 25;
        value ^= value >> 27;
        self.state = value;
        value.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// Next value below `bound` (`bound > 0`).
    pub fn below(&mut self, bound: u32) -> u32 {
        (self.next_u64() % u64::from(bound.max(1))) as u32
    }

    /// Inclusive range `low..=high`.
    pub fn range(&mut self, low: u32, high: u32) -> u32 {
        if high <= low {
            return low;
        }
        low + self.below(high - low + 1)
    }
}

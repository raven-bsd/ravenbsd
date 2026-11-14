// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! xoshiro256** seeded through SplitMix64.
//!
//! Both algorithms are public domain; this is a direct implementation from the published
//! algorithms. The vectors are the C35 harness vectors, so the sweep stream stays comparable.

/// xoshiro256** with a SplitMix64 initialiser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rng {
    state: [u64; 4],
}

impl Rng {
    /// Seed the generator.
    #[must_use]
    pub fn seed_from_u64(seed: u64) -> Self {
        let mut state = seed;
        let first = splitmix64(&mut state);
        let second = splitmix64(&mut state);
        let third = splitmix64(&mut state);
        let fourth = splitmix64(&mut state);
        Self {
            state: [first, second, third, fourth],
        }
    }

    /// Next 64-bit output.
    pub fn next_u64(&mut self) -> u64 {
        let result = self.state[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let shift = self.state[1] << 17;
        self.state[2] ^= self.state[0];
        self.state[3] ^= self.state[1];
        self.state[1] ^= self.state[2];
        self.state[0] ^= self.state[3];
        self.state[2] ^= shift;
        self.state[3] = self.state[3].rotate_left(45);
        result
    }

    /// A value in `0..n`; `n == 0` returns 0.
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            return 0;
        }
        self.next_u64() % n
    }

    /// `n` deterministic bytes.
    pub fn bytes(&mut self, n: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(n);
        while out.len() < n {
            let value = self.next_u64().to_le_bytes();
            let take = (n - out.len()).min(8);
            out.extend_from_slice(&value[..take]);
        }
        out
    }
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut value = *state;
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

#[cfg(test)]
#[path = "rng_ut.rs"]
mod tests;

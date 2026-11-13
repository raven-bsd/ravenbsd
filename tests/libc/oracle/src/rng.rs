// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic pseudo-random generator.
//!
//! xoshiro256** seeded through SplitMix64. Both algorithms are public domain; this is a direct
//! implementation from the published algorithm, not a wrapper around a crate. The test vectors in
//! [`rng_ut`](../rng_ut/index.html) pin the stream so a change cannot silently alter every seed.

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

    /// A value in `0..n` (modulo-biased, which is fine for generated test inputs).
    ///
    /// `n == 0` returns 0 instead of dividing by zero.
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            return 0;
        }
        self.next_u64() % n
    }

    /// `n` pseudo-random bytes.
    pub fn bytes(&mut self, n: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(n);
        while out.len() < n {
            let value = self.next_u64().to_le_bytes();
            let take = (n - out.len()).min(8);
            out.extend_from_slice(&value[..take]);
        }
        out
    }

    /// Pick one element.
    ///
    /// # Panics
    ///
    /// Panics when `items` is empty.
    pub fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        assert!(!items.is_empty(), "pick from an empty slice");
        items[self.below(items.len() as u64) as usize]
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

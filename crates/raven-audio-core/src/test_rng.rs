// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic pseudo-random numbers for the property-style tests.
//!
//! The seed is fixed so a failure can always be reproduced. This module is test-only; the
//! library itself never generates randomness.

/// A xorshift64* generator with a caller-chosen seed.
pub(crate) struct Rng(u64);

impl Rng {
    /// Create a generator. The seed is forced non-zero because xorshift cannot leave zero.
    pub(crate) const fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    /// Next raw 64-bit value.
    pub(crate) fn next_u64(&mut self) -> u64 {
        let mut state = self.0;
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        self.0 = state;
        state.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// A value in `0..bound`; `bound` must be greater than zero.
    pub(crate) fn below(&mut self, bound: u32) -> u32 {
        (self.next_u64() % u64::from(bound)) as u32
    }

    /// An arbitrary PCM16 sample including both extremes.
    pub(crate) fn sample(&mut self) -> i16 {
        (self.next_u64() >> 48) as u16 as i16
    }

    /// A sample biased towards the extremes, which is where overflow hides.
    pub(crate) fn extreme_sample(&mut self) -> i16 {
        match self.below(4) {
            0 => i16::MIN,
            1 => i16::MAX,
            2 => -(self.below(64) as i16 + 1),
            _ => self.below(64) as i16,
        }
    }

    /// `true` about half the time.
    pub(crate) fn coin(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }

    /// Fill a buffer with arbitrary bytes.
    pub(crate) fn fill(&mut self, buffer: &mut [u8]) {
        for byte in buffer {
            *byte = self.next_u64() as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sequence_is_reproducible() {
        let mut first = Rng::new(7);
        let mut second = Rng::new(7);
        for _ in 0..64 {
            assert_eq!(first.next_u64(), second.next_u64());
        }
    }

    #[test]
    fn values_stay_in_range() {
        let mut rng = Rng::new(11);
        for bound in [1, 2, 7, 100, 65_536] {
            for _ in 0..100 {
                assert!(rng.below(bound) < bound, "bound {bound}");
            }
        }
        assert!(rng.coin() || !rng.coin());
    }
}

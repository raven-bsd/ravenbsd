// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic xorshift64* generator for the model seeds.

/// A small deterministic generator; every model run uses fixed seeds.
pub struct Rng(u64);

impl Rng {
    /// Build a generator from a non-zero seed.
    pub const fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    /// Next 64-bit value.
    pub fn next_u64(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value >> 12;
        value ^= value << 25;
        value ^= value >> 27;
        self.0 = value;
        value.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// A value in `0..limit`; `0` when the limit is zero.
    pub fn below(&mut self, limit: u64) -> u64 {
        if limit == 0 {
            0
        } else {
            self.next_u64() % limit
        }
    }

    /// A coin flip.
    pub fn coin(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }

    /// An inclusive `lo..=hi` value; `lo` when the range is inverted or empty.
    pub fn range(&mut self, lo: u32, hi: u32) -> u32 {
        if hi <= lo {
            return lo;
        }
        let span = u64::from(hi - lo) + 1;
        lo + u32::try_from(self.below(span)).unwrap_or(0)
    }
}

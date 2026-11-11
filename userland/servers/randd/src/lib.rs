// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure random-source bookkeeping for `randd`.
//!
//! The server re-seeds its ChaCha20 generator every 64 KiB from the monotonic clock. This is a
//! testable policy module; the actual clock read and the device handler live in the target
//! binary. The output is **not cryptographically secure** until a real entropy source exists.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use raven_prng::{Generator, seed_from_entropy};

/// Re-seed interval in bytes.
pub const RESEED_BYTES: usize = 64 * 1024;

/// A generator plus the clock-based re-seed policy.
pub struct Source {
    generator: Generator,
    salt: usize,
    since_reseed: usize,
}

impl Source {
    /// Seed from a clock value and a stack address.
    pub fn new(clock_ns: u64, stack_addr: usize) -> Self {
        Self {
            generator: Generator::from_entropy(clock_ns, stack_addr),
            salt: stack_addr,
            since_reseed: 0,
        }
    }

    /// Fill `out`; re-seeds first when the interval has elapsed.
    pub fn fill(&mut self, out: &mut [u8], clock_ns: u64) -> usize {
        if self.since_reseed >= RESEED_BYTES {
            self.generator
                .reseed(&seed_from_entropy(clock_ns, self.salt));
            self.since_reseed = 0;
        }
        self.generator.fill(out);
        self.since_reseed = self.since_reseed.saturating_add(out.len());
        out.len()
    }

    /// Mix written bytes into the generator state.
    pub fn mix(&mut self, input: &[u8]) {
        self.generator.mix(input);
    }

    /// Bytes produced since the last re-seed.
    pub fn bytes_since_reseed(&self) -> usize {
        self.since_reseed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_returns_the_requested_length() {
        let mut source = Source::new(1, 0x1000);
        let mut buffer = [0u8; 32];
        assert_eq!(source.fill(&mut buffer, 2), 32);
    }

    #[test]
    fn same_seed_is_deterministic() {
        let mut first = Source::new(7, 0x2000);
        let mut second = Source::new(7, 0x2000);
        let mut left = [0u8; 64];
        let mut right = [0u8; 64];
        first.fill(&mut left, 1);
        second.fill(&mut right, 1);
        assert_eq!(left, right);
    }

    #[test]
    fn different_entropy_differs() {
        let mut first = Source::new(1, 0x1000);
        let mut second = Source::new(2, 0x1000);
        let mut left = [0u8; 64];
        let mut right = [0u8; 64];
        first.fill(&mut left, 1);
        second.fill(&mut right, 1);
        assert_ne!(left, right);
    }

    #[test]
    fn byte_counter_advances() {
        let mut source = Source::new(1, 0x1000);
        let mut buffer = [0u8; 16];
        source.fill(&mut buffer, 1);
        assert_eq!(source.bytes_since_reseed(), 16);
        source.fill(&mut buffer, 2);
        assert_eq!(source.bytes_since_reseed(), 32);
    }

    #[test]
    fn reseed_after_the_interval() {
        let mut source = Source::new(1, 0x1000);
        let mut buffer = vec![0u8; RESEED_BYTES];
        source.fill(&mut buffer, 1);
        assert_eq!(source.bytes_since_reseed(), RESEED_BYTES);
        let mut one = [0u8; 1];
        source.fill(&mut one, 2);
        assert_eq!(source.bytes_since_reseed(), 1);
    }

    #[test]
    fn mix_changes_output() {
        let mut mixed = Source::new(1, 0x1000);
        mixed.mix(b"write");
        let mut mixed_out = [0u8; 32];
        mixed.fill(&mut mixed_out, 1);

        let mut plain = Source::new(1, 0x1000);
        let mut plain_out = [0u8; 32];
        plain.fill(&mut plain_out, 1);
        assert_ne!(mixed_out, plain_out);
    }

    #[test]
    fn empty_fill_does_not_move_the_counter() {
        let mut source = Source::new(1, 0x1000);
        source.fill(&mut [], 1);
        assert_eq!(source.bytes_since_reseed(), 0);
    }

    #[test]
    fn large_output_is_not_all_zero() {
        let mut source = Source::new(1, 0x1000);
        let mut buffer = vec![0u8; 4096];
        source.fill(&mut buffer, 1);
        assert!(buffer.iter().any(|byte| *byte != 0));
    }
}

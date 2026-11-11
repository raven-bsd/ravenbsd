// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! ChaCha20-based pseudo-random generator for `randd`.
//!
//! The generator uses the ChaCha20 block function with a 256-bit key and a 96-bit nonce. It is
//! suitable for device-file randomness and test vectors, **not** for cryptographic secrets:
//! `randd` seeds it from the monotonic clock and a stack address and re-seeds it from the clock
//! every 64 KiB until a real entropy source exists (K41/B47).
//!
//! The crate is `no_std`, allocation-free, dependency-free and contains no `unsafe` code.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// ChaCha20 block size in bytes.
pub const BLOCK_SIZE: usize = 64;

const SIGMA: [u32; 4] = [0x6170_7865, 0x3320_646e, 0x7962_2d32, 0x6b20_6574];

/// Compute one ChaCha20 block.
///
/// The round function follows RFC 8439: 20 rounds (10 column/diagonal double rounds), the
/// original state added at the end and the 16 words serialized little-endian.
pub fn block(key: &[u8; 32], counter: u32, nonce: &[u8; 12], out: &mut [u8; BLOCK_SIZE]) {
    let mut state = [0u32; 16];
    state[0..4].copy_from_slice(&SIGMA);
    for (index, word) in key.chunks_exact(4).enumerate() {
        let bytes = [word[0], word[1], word[2], word[3]];
        state[4 + index] = u32::from_le_bytes(bytes);
    }
    state[12] = counter;
    for (index, word) in nonce.chunks_exact(4).enumerate() {
        let bytes = [word[0], word[1], word[2], word[3]];
        state[13 + index] = u32::from_le_bytes(bytes);
    }

    let initial = state;
    for _ in 0..10 {
        quarter(&mut state, 0, 4, 8, 12);
        quarter(&mut state, 1, 5, 9, 13);
        quarter(&mut state, 2, 6, 10, 14);
        quarter(&mut state, 3, 7, 11, 15);
        quarter(&mut state, 0, 5, 10, 15);
        quarter(&mut state, 1, 6, 11, 12);
        quarter(&mut state, 2, 7, 8, 13);
        quarter(&mut state, 3, 4, 9, 14);
    }
    for (index, word) in state.iter_mut().enumerate() {
        *word = word.wrapping_add(initial[index]);
    }
    for (index, word) in state.iter().enumerate() {
        out[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
}

fn quarter(state: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    state[a] = state[a].wrapping_add(state[b]);
    state[d] = (state[d] ^ state[a]).rotate_left(16);
    state[c] = state[c].wrapping_add(state[d]);
    state[b] = (state[b] ^ state[c]).rotate_left(12);
    state[a] = state[a].wrapping_add(state[b]);
    state[d] = (state[d] ^ state[a]).rotate_left(8);
    state[c] = state[c].wrapping_add(state[d]);
    state[b] = (state[b] ^ state[c]).rotate_left(7);
}

/// Derive a 32-byte seed from a clock value and a stack address.
///
/// This is a SplitMix64 expansion, not a cryptographic KDF. It exists so `randd` can start
/// without an entropy source; see the crate note.
pub fn seed_from_entropy(clock_ns: u64, stack_addr: usize) -> [u8; 32] {
    let mut state = clock_ns ^ (stack_addr as u64).rotate_left(32) ^ 0x9e37_79b9_7f4a_7c15;
    let mut seed = [0u8; 32];
    for chunk in seed.chunks_exact_mut(8) {
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        chunk.copy_from_slice(&splitmix64(state).to_le_bytes());
    }
    seed
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut mixed = value;
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    mixed ^ (mixed >> 31)
}

/// A buffered ChaCha20 stream generator.
pub struct Generator {
    key: [u8; 32],
    nonce: [u8; 12],
    counter: u32,
    buffer: [u8; BLOCK_SIZE],
    used: usize,
}

impl Generator {
    /// Create a generator from an explicit key and nonce. The first block uses counter 0.
    pub fn from_parts(key: [u8; 32], nonce: [u8; 12]) -> Self {
        Self {
            key,
            nonce,
            counter: 0,
            buffer: [0u8; BLOCK_SIZE],
            used: BLOCK_SIZE,
        }
    }

    /// Create a generator from a 32-byte seed; the nonce is derived from the same seed.
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let mut nonce = [0u8; 12];
        nonce.copy_from_slice(&seed[..12]);
        Self::from_parts(*seed, nonce)
    }

    /// Create a generator from the clock and a stack address.
    pub fn from_entropy(clock_ns: u64, stack_addr: usize) -> Self {
        Self::from_seed(&seed_from_entropy(clock_ns, stack_addr))
    }

    /// Replace the key and nonce and restart at counter 0.
    pub fn reseed(&mut self, seed: &[u8; 32]) {
        self.key = *seed;
        self.nonce.copy_from_slice(&seed[..12]);
        self.counter = 0;
        self.used = BLOCK_SIZE;
    }

    /// Mix caller bytes into the key. This is not a cryptographic update; it keeps repeated
    /// writes from producing the same stream.
    pub fn mix(&mut self, input: &[u8]) {
        if input.is_empty() {
            return;
        }
        for (index, byte) in input.iter().enumerate() {
            self.key[index % self.key.len()] ^= byte.rotate_left((index % 8) as u32);
        }
        self.used = BLOCK_SIZE;
    }

    /// Fill `out` with the next bytes of the stream.
    pub fn fill(&mut self, out: &mut [u8]) {
        let mut written = 0usize;
        while written < out.len() {
            if self.used == BLOCK_SIZE {
                block(&self.key, self.counter, &self.nonce, &mut self.buffer);
                self.counter = self.counter.wrapping_add(1);
                self.used = 0;
            }
            let available = BLOCK_SIZE - self.used;
            let remaining = out.len() - written;
            let take = available.min(remaining);
            out[written..written + take].copy_from_slice(&self.buffer[self.used..self.used + take]);
            self.used += take;
            written += take;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const KEY: [u8; 32] = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
        0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d,
        0x1e, 0x1f,
    ];
    const NONCE: [u8; 12] = [
        0x00, 0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0x4a, 0x00, 0x00, 0x00, 0x00,
    ];
    const RFC_BLOCK: [u8; 64] = [
        0x10, 0xf1, 0xe7, 0xe4, 0xd1, 0x3b, 0x59, 0x15, 0x50, 0x0f, 0xdd, 0x1f, 0xa3, 0x20, 0x71,
        0xc4, 0xc7, 0xd1, 0xf4, 0xc7, 0x33, 0xc0, 0x68, 0x03, 0x04, 0x22, 0xaa, 0x9a, 0xc3, 0xd4,
        0x6c, 0x4e, 0xd2, 0x82, 0x64, 0x46, 0x07, 0x9f, 0xaa, 0x09, 0x14, 0xc2, 0xd7, 0x05, 0xd9,
        0x8b, 0x02, 0xa2, 0xb5, 0x12, 0x9c, 0xd1, 0xde, 0x16, 0x4e, 0xb9, 0xcb, 0xd0, 0x83, 0xe8,
        0xa2, 0x50, 0x3c, 0x4e,
    ];

    #[test]
    fn rfc_8439_block_known_answer() {
        let mut out = [0u8; 64];
        block(&KEY, 1, &NONCE, &mut out);
        assert_eq!(out, RFC_BLOCK);
    }

    #[test]
    fn stream_starts_with_the_first_block() {
        let mut generator = Generator::from_parts(KEY, NONCE);
        let mut out = [0u8; 64];
        generator.fill(&mut out);
        let mut expected = [0u8; 64];
        block(&KEY, 0, &NONCE, &mut expected);
        assert_eq!(out, expected);
    }

    #[test]
    fn same_seed_is_deterministic() {
        let mut first = Generator::from_seed(&[7u8; 32]);
        let mut second = Generator::from_seed(&[7u8; 32]);
        let mut left = [0u8; 100];
        let mut right = [0u8; 100];
        first.fill(&mut left);
        second.fill(&mut right);
        assert_eq!(left, right);
    }

    #[test]
    fn different_seeds_differ() {
        let mut first = Generator::from_seed(&[1u8; 32]);
        let mut second = Generator::from_seed(&[2u8; 32]);
        let mut left = [0u8; 64];
        let mut right = [0u8; 64];
        first.fill(&mut left);
        second.fill(&mut right);
        assert_ne!(left, right);
    }

    #[test]
    fn short_fills_match_one_long_fill() {
        let mut split = Generator::from_parts(KEY, NONCE);
        let mut split_output = [0u8; 96];
        let mut first = [0u8; 17];
        let mut second = [0u8; 79];
        split.fill(&mut first);
        split.fill(&mut second);
        split_output[..17].copy_from_slice(&first);
        split_output[17..].copy_from_slice(&second);

        let mut whole = Generator::from_parts(KEY, NONCE);
        let mut whole_output = [0u8; 96];
        whole.fill(&mut whole_output);
        assert_eq!(split_output, whole_output);
    }

    #[test]
    fn stream_continues_across_blocks() {
        let mut generator = Generator::from_parts(KEY, NONCE);
        let mut output = [0u8; 128];
        generator.fill(&mut output);
        let mut first = [0u8; 64];
        block(&KEY, 0, &NONCE, &mut first);
        assert_eq!(&output[..64], &first);
        let mut next = [0u8; 64];
        block(&KEY, 1, &NONCE, &mut next);
        assert_eq!(&output[64..], &next);
    }

    #[test]
    fn counter_increments() {
        let mut first = [0u8; 64];
        let mut second = [0u8; 64];
        block(&KEY, 0, &NONCE, &mut first);
        block(&KEY, 1, &NONCE, &mut second);
        assert_ne!(first, second);
    }

    #[test]
    fn reseed_changes_the_stream() {
        let mut generator = Generator::from_parts(KEY, NONCE);
        let mut before = [0u8; 64];
        generator.fill(&mut before);
        generator.reseed(&[9u8; 32]);
        let mut after = [0u8; 64];
        generator.fill(&mut after);
        assert_ne!(before, after);
    }

    #[test]
    fn mix_changes_the_stream() {
        let mut mixed = Generator::from_parts(KEY, NONCE);
        mixed.mix(b"input");
        let mut mixed_output = [0u8; 64];
        mixed.fill(&mut mixed_output);

        let mut plain = Generator::from_parts(KEY, NONCE);
        let mut plain_output = [0u8; 64];
        plain.fill(&mut plain_output);
        assert_ne!(mixed_output, plain_output);
    }

    #[test]
    fn empty_mix_is_a_no_op() {
        let mut mixed = Generator::from_parts(KEY, NONCE);
        mixed.mix(&[]);
        let mut mixed_output = [0u8; 64];
        mixed.fill(&mut mixed_output);
        let mut expected = [0u8; 64];
        block(&KEY, 0, &NONCE, &mut expected);
        assert_eq!(mixed_output, expected);
    }

    #[test]
    fn empty_fill_is_a_no_op() {
        let mut generator = Generator::from_parts(KEY, NONCE);
        generator.fill(&mut []);
        let mut output = [0u8; 64];
        generator.fill(&mut output);
        let mut expected = [0u8; 64];
        block(&KEY, 0, &NONCE, &mut expected);
        assert_eq!(output, expected);
    }

    #[test]
    fn blocks_do_not_repeat() {
        let mut generator = Generator::from_parts(KEY, NONCE);
        let mut seen = HashSet::new();
        let mut output = [0u8; 64];
        for _ in 0..512 {
            generator.fill(&mut output);
            assert!(seen.insert(output), "a 64-byte block repeated");
        }
    }

    #[test]
    fn seed_from_entropy_is_deterministic() {
        assert_eq!(
            seed_from_entropy(1234, 0x1000),
            seed_from_entropy(1234, 0x1000)
        );
    }

    #[test]
    fn seed_from_entropy_tracks_both_inputs() {
        assert_ne!(
            seed_from_entropy(1234, 0x1000),
            seed_from_entropy(1235, 0x1000)
        );
        assert_ne!(
            seed_from_entropy(1234, 0x1000),
            seed_from_entropy(1234, 0x2000)
        );
    }

    #[test]
    fn from_entropy_is_repeatable() {
        let mut first = Generator::from_entropy(42, 0x4000);
        let mut second = Generator::from_entropy(42, 0x4000);
        let mut left = [0u8; 96];
        let mut right = [0u8; 96];
        first.fill(&mut left);
        second.fill(&mut right);
        assert_eq!(left, right);
    }

    #[test]
    fn large_read_contains_distinct_blocks() {
        let mut generator = Generator::from_seed(&[3u8; 32]);
        let mut data = vec![0u8; BLOCK_SIZE * 64];
        generator.fill(&mut data);
        let blocks: HashSet<&[u8]> = data.chunks_exact(BLOCK_SIZE).collect();
        assert_eq!(blocks.len(), 64);
    }
}

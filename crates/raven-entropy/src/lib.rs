// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure entropy pool for RavenBSD.
//!
//! The pool collects seed material from a hardware entropy source (RDRAND, RNDR or the Raspberry
//! Pi 4 RNG200) and produces random bytes from a ChaCha20 stream, re-keyed whenever the caller
//! supplies fresh material and refused past a bounded output budget so the stream can never run
//! unseeded. It is a *model* of the kernel entropy path: no hardware, no blocking, no allocation.
//!
//! # Construction and threat model
//!
//! * **Primitive**: the ChaCha20 block function (RFC 8439), the only cryptographic component.
//!   It is an admitted, widely reviewed construction; this crate does not invent a cipher and is
//!   verified against the RFC 8439 known-answer test in the unit tests.
//! * **Accumulator**: caller material is folded into a 64-byte accumulator. When it fills, one
//!   ChaCha20 block (keyed by the current key, with the accumulator as nonce/counter) is folded
//!   into a fresh 32-byte key and the accumulator resets. This is a conservative ChaCha20-based
//!   key update, not a custom mix.
//! * **Output**: [`Pool::fill`] streams ChaCha20 keystream. The pool enforces a reseed budget
//!   ([`RESEED_INTERVAL`]); when it is exhausted, [`Error::NeedsReseed`] forces the caller to add
//!   more entropy, and an unseeded pool returns [`Error::Unseeded`].
//! * **Threat model**: output is exactly as strong as the seed material. Seeded from a hardware
//!   DRNG (RDRAND/RNDR/RNG200) it inherits that device's strength; seeded from timer jitter only
//!   it is **not** cryptographically secure (jitter is a weak, possibly observable source) and is
//!   used only so boot and early userspace never block. The caller, not this crate, names the
//!   source; this crate never blocks and never claims more than the seed provides.

#![cfg_attr(not(target_os = "none"), no_std)]
#![forbid(unsafe_code)]
// When compiled into the kernel (target_os = "none") the host-test-only public surface is unused.
#![cfg_attr(target_os = "none", allow(dead_code))]
// The ChaCha20 block function indexes a fixed 16-word state with literal round indices and folds
// fixed-size byte arrays; every index and offset is compile-time-bounded, so the kernel's
// `indexing_slicing`/`arithmetic_side_effects` lints do not apply to this pure, host-tested math.
#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

#[cfg(test)]
extern crate std;

/// ChaCha20 block size in bytes.
pub const BLOCK_SIZE: usize = 64;
/// The key length (ChaCha20 uses a 256-bit key).
pub const KEY_SIZE: usize = 32;
/// The nonce length used by the key update.
pub const NONCE_SIZE: usize = 12;
/// Accumulator size: one ChaCha20 block of seed material.
pub const ACCUMULATOR_SIZE: usize = 64;
/// Output bytes allowed between re-keyings. Exceeding this forces [`Error::NeedsReseed`].
pub const RESEED_INTERVAL: u64 = 65_536;

const SIGMA: [u32; 4] = [0x6170_7865, 0x3320_646e, 0x7962_2d32, 0x6b20_6574];

/// Why [`Pool::fill`] refused to produce bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The pool has never absorbed enough seed material.
    Unseeded,
    /// The pool has emitted [`RESEED_INTERVAL`] bytes since the last re-keying.
    NeedsReseed,
}

/// One quarter round of the ChaCha20 block function (RFC 8439 section 2.1).
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

/// Compute one ChaCha20 block (RFC 8439): 20 rounds, add the initial state, serialize
/// little-endian into `out`.
pub fn block(
    key: &[u8; KEY_SIZE],
    counter: u32,
    nonce: &[u8; NONCE_SIZE],
    out: &mut [u8; BLOCK_SIZE],
) {
    let mut state = [0u32; 16];
    state[0..4].copy_from_slice(&SIGMA);
    for (index, word) in key.chunks_exact(4).enumerate() {
        state[4 + index] = u32::from_le_bytes([word[0], word[1], word[2], word[3]]);
    }
    state[12] = counter;
    for (index, word) in nonce.chunks_exact(4).enumerate() {
        state[13 + index] = u32::from_le_bytes([word[0], word[1], word[2], word[3]]);
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

/// The entropy pool.
///
/// Fixed-size and allocation-free; safe to place in a static and guard with an interrupt-safe
/// lock in the kernel. Every method takes `&mut self`.
pub struct Pool {
    key: [u8; KEY_SIZE],
    counter: u32,
    accumulator: [u8; ACCUMULATOR_SIZE],
    acc_len: usize,
    since_reseed: u64,
    seeded: bool,
}

impl Pool {
    /// Create an unseeded pool.
    #[must_use]
    pub fn new() -> Self {
        Self {
            key: [0u8; KEY_SIZE],
            counter: 0,
            accumulator: [0u8; ACCUMULATOR_SIZE],
            acc_len: 0,
            since_reseed: 0,
            seeded: false,
        }
    }

    /// Whether the pool has absorbed a full accumulator of seed material.
    #[must_use]
    pub fn is_seeded(&self) -> bool {
        self.seeded
    }

    /// Fold `material` into the accumulator; re-key once a full block has accumulated.
    ///
    /// The fold is positional (byte `i` of the material lands at the accumulator position it is
    /// appended to), so feeding the same bytes in any number of pieces yields the same state.
    pub fn add_entropy(&mut self, material: &[u8]) {
        let mut index = 0usize;
        while index < material.len() {
            let available = ACCUMULATOR_SIZE - self.acc_len;
            let take = available.min(material.len() - index);
            for (offset, byte) in material[index..index + take].iter().enumerate() {
                self.accumulator[self.acc_len + offset] ^= byte;
            }
            self.acc_len += take;
            index += take;
            if self.acc_len == ACCUMULATOR_SIZE {
                self.rekey();
            }
        }
    }

    /// Fill `out` with pool bytes.
    ///
    /// Returns [`Error::Unseeded`] when no seed material has been absorbed and
    /// [`Error::NeedsReseed`] when filling would exceed [`RESEED_INTERVAL`] since the last
    /// re-keying. On success the whole `out` is filled.
    pub fn fill(&mut self, out: &mut [u8]) -> Result<usize, Error> {
        if !self.seeded {
            return Err(Error::Unseeded);
        }
        let budget = RESEED_INTERVAL - self.since_reseed.min(RESEED_INTERVAL);
        if out.len() as u64 > budget {
            return Err(Error::NeedsReseed);
        }
        let nonce = [0u8; NONCE_SIZE];
        let mut written = 0usize;
        while written < out.len() {
            let mut buffer = [0u8; BLOCK_SIZE];
            block(&self.key, self.counter, &nonce, &mut buffer);
            self.counter = self.counter.wrapping_add(1);
            let take = (out.len() - written).min(BLOCK_SIZE);
            out[written..written + take].copy_from_slice(&buffer[..take]);
            written += take;
        }
        self.since_reseed += out.len() as u64;
        Ok(out.len())
    }

    /// Re-key from the accumulator and restart the stream.
    fn rekey(&mut self) {
        let mut nonce = [0u8; NONCE_SIZE];
        nonce.copy_from_slice(&self.accumulator[..NONCE_SIZE]);
        let mut buffer = [0u8; BLOCK_SIZE];
        block(&self.key, 0, &nonce, &mut buffer);
        // Fold the 64-byte block into a 32-byte key.
        for index in 0..KEY_SIZE {
            self.key[index] = buffer[index] ^ buffer[index + KEY_SIZE];
        }
        self.accumulator = [0u8; ACCUMULATOR_SIZE];
        self.acc_len = 0;
        self.counter = 0;
        self.since_reseed = 0;
        self.seeded = true;
    }
}

impl Default for Pool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

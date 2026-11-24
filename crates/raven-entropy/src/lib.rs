// SPDX-License-Identifier: ISC
// Copyright (c) 1996, David Mazieres <dm@uun.org>
// Copyright (c) 2008, Damien Miller <djm@openbsd.org>
// Copyright (c) 2013, Markus Friedl <markus@openbsd.org>
// Copyright (c) 2014, Theo de Raadt <deraadt@openbsd.org>
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/crypt/arc4random.c,
// arc4random.h and chacha_private.h
// @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tags: $OpenBSD: arc4random.c,v 1.58 2022/07/31 13:41:45 tb Exp $;
//  arc4random.h,v 1.4 2015/01/15 06:57:18 deraadt Exp $;
//  chacha_private.h,v 1.3 2022/02/28 21:56:29 dtucker Exp $)
//
// Permission to use, copy, modify, and distribute this software for any
// purpose with or without fee is hereby granted, provided that the above
// copyright notice and this permission notice appear in all copies.
//
// THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
// WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
// MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
// ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
// WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
// ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
// OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
//
// The ChaCha block function in `chacha_private.h` carries this notice:
// "chacha-merged.c version 20080118, D. J. Bernstein, Public domain."
//
// Raven port: the construction and ChaCha state are transliterated; `getentropy`,
// `mmap`/`minherit(MAP_INHERIT_ZERO)` and the libc lock are replaced by the
// explicit seed source, [`Pool::fork_child`] and the caller's lock. See the
// crate documentation for the mapping.

//! OpenBSD `arc4random` construction for RavenBSD (host-tested).
//!
//! The shape follows OpenBSD `lib/libc/crypt/arc4random.c`, `arc4random.h` and
//! `chacha_private.h` (ISC): a ChaCha20 keystream buffer of 1,024 bytes, a 40-byte seed
//! (32-byte key + 8-byte IV), a randomized re-key interval near 1 MiB and a fork-zeroed state.
//! The keystream is generated with ChaCha20 only; the RFC 8439 known-answer test below is the
//! primitive oracle.
//!
//! Rust translation notes:
//!
//! * `_rs`/`_rsx`, `_rs_init`, `_rs_stir`, `_rs_rekey` and `_rs_random_buf` map to [`Pool`],
//!   `ChaCha`, `stir`, `rekey` and `fill`. The ChaCha state keeps the 8-byte IV and 64-bit
//!   counter of `chacha_ivsetup`.
//! * OpenBSD's `_rs_allocate` uses `mmap` + `minherit(MAP_INHERIT_ZERO)`; this pure crate cannot
//!   call either, so [`Pool::fork_child`] returns a zeroed pool and [`Pool::stir`] must be called
//!   in the child before any output. That is the same observable rule (child state is erased and
//!   reseeded, parent keystream is not inherited).
//! * On exhaustion [`Pool::fill`] returns [`Error::NeedsReseed`]; the caller obtains a fresh
//!   `getentropy`/hardware seed and calls [`Pool::stir`]. OpenBSD calls `getentropy` itself; the
//!   pure crate has no libc, so the reseed is explicit.
//! * `getentropy` is the caller's job; [`Pool::stir`] accepts only `GetEntropy`/hardware seeds.
//!   `SeedSource::TimerJitter` is a defined error, never a cryptographic seed.
//!
//! The crate is `no_std`, allocation-free outside tests and `forbid(unsafe_code)`.
//!
//! Zeroization is explicit (`fill(0)` plus `core::hint::black_box`). Safe Rust cannot issue
//! OpenBSD `explicit_bzero`'s volatile stores; the wipe tests observe the state, and the
//! compiler is discouraged from eliding the stores. This is recorded as an open gap in the
//! K523 report.

#![cfg_attr(not(target_os = "none"), no_std)]
#![forbid(unsafe_code)]
#![cfg_attr(target_os = "none", allow(dead_code))]
#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

#[cfg(test)]
extern crate std;

/// ChaCha20 block size.
pub const BLOCK_SIZE: usize = 64;
/// ChaCha20 key length.
pub const KEY_SIZE: usize = 32;
/// OpenBSD `IVSZ`: the original ChaCha 8-byte IV.
pub const IV_SIZE: usize = 8;
/// OpenBSD seed: 32-byte key + 8-byte IV.
pub const SEED_SIZE: usize = KEY_SIZE + IV_SIZE;
/// OpenBSD `RSBUFSZ`: 16 keystream blocks.
pub const RS_BUFSZ: usize = 16 * BLOCK_SIZE;
/// OpenBSD `REKEY_BASE`: 1 MiB. The actual interval is `REKEY_BASE + fuzz % REKEY_BASE`.
pub const REKEY_BASE: u64 = 1024 * 1024;
/// Compatibility alias for the old pool's public constant.
pub const RESEED_INTERVAL: u64 = REKEY_BASE;
/// RFC 8439 12-byte nonce length, used by the primitive KAT.
pub const NONCE_SIZE: usize = 12;

const SIGMA: [u32; 4] = [0x6170_7865, 0x3320_646e, 0x7962_2d32, 0x6b20_6574];

/// Best-effort zeroization: clear the buffer and make the stores observable to the optimizer.
///
/// Safe Rust cannot emit OpenBSD `explicit_bzero`'s volatile stores; `black_box` prevents the
/// compiler from treating the cleared buffer as dead before the next use. The wipe tests observe
/// the resulting state; the residual compiler caveat is recorded in the K523 report.
fn zero(bytes: &mut [u8]) {
    bytes.fill(0);
    core::hint::black_box(bytes);
}

/// Where a seed came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeedSource {
    /// `getentropy(2)` seed material.
    GetEntropy,
    /// Hardware DRNG (RDRAND/RNDR/RNG200).
    Hardware,
    /// Timer jitter is not a cryptographic source and is rejected.
    TimerJitter,
}

/// Why an operation was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// No seed has been installed; call [`Pool::stir`] first.
    Unseeded,
    /// The randomized re-key interval is exhausted; the caller must `stir` a fresh seed.
    NeedsReseed,
    /// The requested source is not a cryptographic seed source.
    WeakSource,
}

/// One quarter round (RFC 8439 section 2.1).
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

/// Compute one RFC 8439 ChaCha20 block with a 96-bit nonce and 32-bit counter.
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
    rounds(&mut state);
    for (index, word) in state.iter().enumerate() {
        out[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
}

/// The 20 rounds and feed-forward for a 16-word state.
fn rounds(state: &mut [u32; 16]) {
    let initial = *state;
    for _ in 0..10 {
        quarter(state, 0, 4, 8, 12);
        quarter(state, 1, 5, 9, 13);
        quarter(state, 2, 6, 10, 14);
        quarter(state, 3, 7, 11, 15);
        quarter(state, 0, 5, 10, 15);
        quarter(state, 1, 6, 11, 12);
        quarter(state, 2, 7, 8, 13);
        quarter(state, 3, 4, 9, 14);
    }
    for (word, base) in state.iter_mut().zip(initial) {
        *word = word.wrapping_add(base);
    }
}

/// Compute one original-ChaCha block with an 8-byte IV and 64-bit counter
/// (`chacha_ivsetup`/`chacha_encrypt_bytes`).
fn block_iv8(key: &[u8; KEY_SIZE], counter: u64, iv: &[u8; IV_SIZE], out: &mut [u8; BLOCK_SIZE]) {
    let mut state = [0u32; 16];
    state[0..4].copy_from_slice(&SIGMA);
    for (index, word) in key.chunks_exact(4).enumerate() {
        state[4 + index] = u32::from_le_bytes([word[0], word[1], word[2], word[3]]);
    }
    state[12] = counter as u32;
    state[13] = (counter >> 32) as u32;
    for (index, word) in iv.chunks_exact(4).enumerate() {
        state[14 + index] = u32::from_le_bytes([word[0], word[1], word[2], word[3]]);
    }
    rounds(&mut state);
    for (index, word) in state.iter().enumerate() {
        out[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
}

/// The OpenBSD ChaCha context: key, IV and the 64-bit block counter.
struct ChaCha {
    key: [u8; KEY_SIZE],
    iv: [u8; IV_SIZE],
    counter: u64,
}

impl ChaCha {
    const fn new() -> Self {
        Self {
            key: [0u8; KEY_SIZE],
            iv: [0u8; IV_SIZE],
            counter: 0,
        }
    }

    fn init(&mut self, seed: &[u8; SEED_SIZE]) {
        self.key.copy_from_slice(&seed[..KEY_SIZE]);
        self.iv.copy_from_slice(&seed[KEY_SIZE..SEED_SIZE]);
        self.counter = 0;
    }

    /// `chacha_encrypt_bytes` with `KEYSTREAM_ONLY`.
    fn keystream(&mut self, out: &mut [u8]) {
        let mut offset = 0usize;
        while offset < out.len() {
            let mut block = [0u8; BLOCK_SIZE];
            block_iv8(&self.key, self.counter, &self.iv, &mut block);
            self.counter = self.counter.wrapping_add(1);
            let take = (out.len() - offset).min(BLOCK_SIZE);
            out[offset..offset + take].copy_from_slice(&block[..take]);
            // The local block is not secret-independent; clear it before the next one.
            zero(&mut block);
            offset += take;
        }
    }

    fn wipe(&mut self) {
        zero(&mut self.key);
        zero(&mut self.iv);
        self.counter = 0;
    }
}

/// The OpenBSD `_rs`/`_rsx` state.
pub struct Pool {
    chacha: ChaCha,
    rs_buf: [u8; RS_BUFSZ],
    rs_have: usize,
    rs_count: u64,
    seeded: bool,
}

impl Pool {
    /// A zeroed, unseeded pool.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            chacha: ChaCha::new(),
            rs_buf: [0u8; RS_BUFSZ],
            rs_have: 0,
            rs_count: 0,
            seeded: false,
        }
    }

    /// Whether [`Pool::stir`] has installed a seed.
    #[must_use]
    pub fn is_seeded(&self) -> bool {
        self.seeded
    }

    /// OpenBSD `_rs_stir`: install a 40-byte seed and choose a randomized re-key interval.
    ///
    /// Timer jitter is refused with [`Error::WeakSource`]; the caller must supply `getentropy` or
    /// hardware seeds.
    pub fn stir(&mut self, source: SeedSource, seed: &[u8; SEED_SIZE]) -> Result<(), Error> {
        if source == SeedSource::TimerJitter {
            return Err(Error::WeakSource);
        }
        self.stir_seed(seed)
    }

    /// Install an explicitly weak software seed through the same OpenBSD `_rs_stir` construction.
    ///
    /// This exists only for the aarch64 boot fallback where no RDRAND/RNDR is available. The
    /// caller must log the weak source and the measured jitter bound; this method makes no
    /// cryptographic-strength claim. [`Pool::stir`] still rejects `SeedSource::TimerJitter`, so
    /// the strict K523 gate is unchanged.
    pub fn stir_weak(&mut self, seed: &[u8; SEED_SIZE]) -> Result<(), Error> {
        self.stir_seed(seed)
    }

    fn stir_seed(&mut self, seed: &[u8; SEED_SIZE]) -> Result<(), Error> {
        if self.seeded {
            self.rekey(Some(seed));
        } else {
            self.chacha.init(seed);
            self.seeded = true;
        }
        // OpenBSD invalidates the keystream buffer after a stir.
        self.rs_have = 0;
        zero(&mut self.rs_buf);
        // Randomize the next interval with four bytes of the fresh stream.
        let mut fuzz = [0u8; 4];
        self.chacha.keystream(&mut fuzz);
        let value = u32::from_le_bytes(fuzz);
        self.rs_count = REKEY_BASE + u64::from(value % (REKEY_BASE as u32));
        zero(&mut fuzz);
        Ok(())
    }

    /// OpenBSD `_rs_random_buf`, except that an exhausted interval returns [`Error::NeedsReseed`]
    /// so the caller supplies a fresh `getentropy` seed instead of silently stretching the stream.
    pub fn fill(&mut self, out: &mut [u8]) -> Result<usize, Error> {
        if !self.seeded {
            return Err(Error::Unseeded);
        }
        if out.is_empty() {
            return Ok(0);
        }
        if out.len() as u64 >= self.rs_count {
            return Err(Error::NeedsReseed);
        }
        self.rs_count -= out.len() as u64;
        let mut written = 0usize;
        while written < out.len() {
            if self.rs_have > 0 {
                let start = RS_BUFSZ - self.rs_have;
                let take = (out.len() - written).min(self.rs_have);
                out[written..written + take].copy_from_slice(&self.rs_buf[start..start + take]);
                zero(&mut self.rs_buf[start..start + take]);
                self.rs_have -= take;
                written += take;
            }
            if self.rs_have == 0 {
                self.rekey(None);
            }
        }
        Ok(out.len())
    }

    /// OpenBSD `_rs_rekey`: fill the buffer from the current context, mix optional seed material,
    /// re-seed from the first 40 bytes (backtracking resistance), then zero those bytes.
    fn rekey(&mut self, data: Option<&[u8; SEED_SIZE]>) {
        self.chacha.keystream(&mut self.rs_buf);
        if let Some(data) = data {
            for (slot, byte) in self.rs_buf[..SEED_SIZE].iter_mut().zip(data) {
                *slot ^= byte;
            }
        }
        let mut seed = [0u8; SEED_SIZE];
        seed.copy_from_slice(&self.rs_buf[..SEED_SIZE]);
        self.chacha.init(&seed);
        zero(&mut seed);
        zero(&mut self.rs_buf[..SEED_SIZE]);
        self.rs_have = RS_BUFSZ - SEED_SIZE;
    }

    /// The child half of OpenBSD's `MAP_INHERIT_ZERO`: a fork child cannot continue the parent
    /// keystream, so it starts zeroed and unseeded. The parent is unchanged.
    #[must_use]
    pub fn fork_child(&self) -> Self {
        Self::new()
    }

    /// Explicitly wipe key, IV, keystream and counters. Called before reuse and on drop.
    pub fn wipe(&mut self) {
        self.chacha.wipe();
        zero(&mut self.rs_buf);
        self.rs_have = 0;
        self.rs_count = 0;
        self.seeded = false;
    }

    /// Legacy adapter for the existing kernel caller: treat the first 40 bytes as a
    /// `getentropy` seed. New code uses [`Pool::stir`] with an explicit source.
    #[doc(hidden)]
    pub fn add_entropy(&mut self, material: &[u8]) {
        if material.len() < SEED_SIZE {
            return;
        }
        let mut seed = [0u8; SEED_SIZE];
        seed.copy_from_slice(&material[..SEED_SIZE]);
        let _ = self.stir(SeedSource::GetEntropy, &seed);
        zero(&mut seed);
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        self.wipe();
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

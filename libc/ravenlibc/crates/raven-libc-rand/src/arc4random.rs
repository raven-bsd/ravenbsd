// SPDX-License-Identifier: ISC
// Copyright (c) 1996, David Mazieres <dm@uun.org>
// Copyright (c) 2008, Damien Miller <djm@openbsd.org>
// Copyright (c) 2013, Markus Friedl <markus@openbsd.org>
// Copyright (c) 2014, Theo de Raadt <deraadt@openbsd.org>
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/crypt/arc4random.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: arc4random.c,v 1.58 2022/07/31 13:41:45 tb Exp $)
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

//! The `arc4random` state machine.
//!
//! The lock, the `mmap`/`minherit` allocator and `_rs_forkdetect` are removed: the state is
//! caller-owned, the shell owns the lock and [`crate::fork::ForkEvents`] wipes the whole state in
//! the child.

#![allow(clippy::indexing_slicing)] // Fixed RSBUFSZ array with a validated cursor.

use core::hint::black_box;
use core::sync::atomic::Ordering;
use core::sync::atomic::compiler_fence;

use raven_libc_malloc::Entropy;

use crate::chacha::ChaCha;
use crate::fork::ForkEvents;
use crate::platform::RandPlatform;
use crate::uniform::uniform_with;

/// Key size in bytes.
pub const KEYSZ: usize = 32;
/// IV size in bytes.
pub const IVSZ: usize = 8;
/// ChaCha block size in bytes.
pub const BLOCKSZ: usize = 64;
/// Keystream buffer size in bytes.
pub const RSBUFSZ: usize = 16 * BLOCKSZ;
/// Base rekey interval in bytes.
pub const REKEY_BASE: usize = 1024 * 1024;

/// The generator state; a zero value means "not initialised".
#[derive(Clone, Debug)]
pub struct Arc4Random {
    rs_have: usize,
    rs_count: usize,
    chacha: ChaCha,
    buf: [u8; RSBUFSZ],
    inited: bool,
}

impl Arc4Random {
    /// An uninitialised generator.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            rs_have: 0,
            rs_count: 0,
            chacha: ChaCha::zeroed(),
            buf: [0; RSBUFSZ],
            inited: false,
        }
    }

    /// One random `u32`.
    pub fn random_u32<E: Entropy, X: RandPlatform>(&mut self, entropy: &E, platform: &X) -> u32 {
        self.stir_if_needed(4, entropy, platform);
        if self.rs_have < 4 {
            self.rekey(None);
        }
        let start = RSBUFSZ - self.rs_have;
        let mut value = [0u8; 4];
        value.copy_from_slice(&self.buf[start..start + 4]);
        self.buf[start..start + 4].fill(0);
        self.rs_have -= 4;
        u32::from_le_bytes(value)
    }

    /// Fill `out`; a zero-length request does nothing.
    pub fn random_buf<E: Entropy, X: RandPlatform>(
        &mut self,
        entropy: &E,
        platform: &X,
        out: &mut [u8],
    ) {
        if out.is_empty() {
            return;
        }
        let mut remaining = out.len();
        self.stir_if_needed(remaining, entropy, platform);
        let mut written = 0usize;
        while remaining > 0 {
            if self.rs_have > 0 {
                let take = remaining.min(self.rs_have);
                let start = RSBUFSZ - self.rs_have;
                out[written..written + take].copy_from_slice(&self.buf[start..start + take]);
                self.buf[start..start + take].fill(0);
                written += take;
                remaining -= take;
                self.rs_have -= take;
            }
            if self.rs_have == 0 {
                self.rekey(None);
            }
        }
    }

    /// One uniform value below `upper_bound`.
    pub fn uniform<E: Entropy, X: RandPlatform>(
        &mut self,
        entropy: &E,
        platform: &X,
        upper_bound: u32,
    ) -> u32 {
        uniform_with(upper_bound, &mut || self.random_u32(entropy, platform))
    }

    /// Force a reseed on the next draw.
    pub fn stir(&mut self) {
        self.rs_count = 0;
    }

    /// Whether every byte of the state is zero (the child wipe contract).
    #[must_use]
    pub fn is_wiped(&self) -> bool {
        !self.inited
            && self.rs_have == 0
            && self.rs_count == 0
            && self.chacha == ChaCha::zeroed()
            && self.buf.iter().all(|byte| *byte == 0)
    }

    #[cfg(test)]
    pub(crate) fn count(&self) -> usize {
        self.rs_count
    }

    #[cfg(test)]
    pub(crate) fn buffer(&self) -> &[u8] {
        &self.buf
    }

    #[cfg(test)]
    pub(crate) fn key_words(&self) -> [u32; 16] {
        self.chacha.words()
    }

    fn stir_if_needed<E: Entropy, X: RandPlatform>(
        &mut self,
        len: usize,
        entropy: &E,
        platform: &X,
    ) {
        if !self.inited || self.rs_count <= len {
            self.stir_with(entropy, platform);
        }
        if self.rs_count <= len {
            self.rs_count = 0;
        } else {
            self.rs_count -= len;
        }
    }

    fn stir_with<E: Entropy, X: RandPlatform>(&mut self, entropy: &E, platform: &X) {
        let mut random = [0u8; KEYSZ + IVSZ];
        if !entropy.fill(&mut random) {
            platform.no_entropy();
        }
        if !self.inited {
            self.init(&random);
            self.inited = true;
        } else {
            self.rekey(Some(&random));
        }
        wipe(&mut random);
        self.rs_have = 0;
        self.buf.fill(0);
        let mut fuzz = [0u8; 4];
        self.chacha.keystream_bytes(&mut fuzz);
        let value = u32::from_le_bytes(fuzz);
        self.rs_count = REKEY_BASE + usize::try_from(value % (REKEY_BASE as u32)).unwrap_or(0);
    }

    fn init(&mut self, seed: &[u8]) {
        if seed.len() < KEYSZ + IVSZ {
            return;
        }
        let mut key = [0u8; KEYSZ];
        key.copy_from_slice(&seed[..KEYSZ]);
        let mut iv = [0u8; IVSZ];
        iv.copy_from_slice(&seed[KEYSZ..KEYSZ + IVSZ]);
        self.chacha.keysetup(&key);
        self.chacha.ivsetup(&iv);
    }

    fn rekey(&mut self, data: Option<&[u8]>) {
        self.chacha.keystream_bytes(&mut self.buf);
        if let Some(data) = data {
            let limit = data.len().min(KEYSZ + IVSZ);
            let mut index = 0usize;
            while index < limit {
                self.buf[index] ^= data[index];
                index += 1;
            }
        }
        let mut seed = [0u8; KEYSZ + IVSZ];
        seed.copy_from_slice(&self.buf[..KEYSZ + IVSZ]);
        self.init(&seed);
        wipe(&mut seed);
        self.buf[..KEYSZ + IVSZ].fill(0);
        self.rs_have = RSBUFSZ - KEYSZ - IVSZ;
    }
}

impl Default for Arc4Random {
    fn default() -> Self {
        Self::new()
    }
}

impl ForkEvents for Arc4Random {
    fn prefork(&mut self) {}

    fn postfork_parent(&mut self) {}

    fn postfork_child(&mut self) {
        self.rs_have = 0;
        self.rs_count = 0;
        self.chacha = ChaCha::zeroed();
        self.buf.fill(0);
        self.inited = false;
        black_box(&self.buf);
        compiler_fence(Ordering::SeqCst);
    }
}

fn wipe(bytes: &mut [u8]) {
    bytes.fill(0);
    black_box(&*bytes);
    compiler_fence(Ordering::SeqCst);
}

#[cfg(test)]
#[path = "arc4random_ut.rs"]
mod tests;

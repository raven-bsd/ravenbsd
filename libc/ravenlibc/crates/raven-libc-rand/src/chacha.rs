// SPDX-License-Identifier: LicenseRef-PublicDomain
// chacha-merged.c version 20080118
// D. J. Bernstein
// Public domain.
//
// RavenBSD: transpiled from openbsd:lib/libc/crypt/chacha_private.h @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: chacha_private.h,v 1.3 2022/02/28 21:56:29 dtucker Exp $)
// KEYSTREAM_ONLY configuration; the 64-bit counter lives in words 12-13 and the IV in 14-15.

//! The ChaCha20 keystream used by [`crate::arc4random`].

#![allow(clippy::indexing_slicing)] // Fixed 16-word state indexed by constants.

const SIGMA: [u32; 4] = [0x6170_7865, 0x3320_646e, 0x7962_2d32, 0x6b20_6574];

/// The 16-word ChaCha state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChaCha {
    input: [u32; 16],
    buffer: [u8; 64],
    used: usize,
}

impl ChaCha {
    /// All-zero state; a zero value means "not initialised".
    #[must_use]
    pub const fn zeroed() -> Self {
        Self {
            input: [0; 16],
            buffer: [0; 64],
            used: 64,
        }
    }

    /// Set a 256-bit key and the `sigma` constants.
    pub fn keysetup(&mut self, key: &[u8; 32]) {
        self.input[0..4].copy_from_slice(&SIGMA);
        let mut index = 0usize;
        while index < 8 {
            let offset = index.wrapping_mul(4);
            self.input[4 + index] = u32::from_le_bytes([
                key[offset],
                key[offset + 1],
                key[offset + 2],
                key[offset + 3],
            ]);
            index = index.wrapping_add(1);
        }
        self.used = 64;
    }

    /// Set the 64-bit IV and reset the 64-bit counter.
    pub fn ivsetup(&mut self, iv: &[u8; 8]) {
        self.input[12] = 0;
        self.input[13] = 0;
        self.input[14] = u32::from_le_bytes([iv[0], iv[1], iv[2], iv[3]]);
        self.input[15] = u32::from_le_bytes([iv[4], iv[5], iv[6], iv[7]]);
        self.used = 64;
    }

    /// Fill `out` with the next keystream bytes. The counter advances by whole blocks.
    pub fn keystream_bytes(&mut self, out: &mut [u8]) {
        let mut written = 0usize;
        while written < out.len() {
            if self.used == 64 {
                let mut block = [0u8; 64];
                self.block(&mut block);
                self.buffer = block;
                self.used = 0;
            }
            let take = (out.len() - written).min(64 - self.used);
            out[written..written + take].copy_from_slice(&self.buffer[self.used..self.used + take]);
            self.used = self.used.wrapping_add(take);
            written = written.wrapping_add(take);
        }
    }

    #[cfg(test)]
    pub(crate) fn words(&self) -> [u32; 16] {
        self.input
    }

    fn block(&mut self, out: &mut [u8; 64]) {
        let initial = self.input;
        let mut x = initial;
        let mut round = 0usize;
        while round < 10 {
            quarter(&mut x, 0, 4, 8, 12);
            quarter(&mut x, 1, 5, 9, 13);
            quarter(&mut x, 2, 6, 10, 14);
            quarter(&mut x, 3, 7, 11, 15);
            quarter(&mut x, 0, 5, 10, 15);
            quarter(&mut x, 1, 6, 11, 12);
            quarter(&mut x, 2, 7, 8, 13);
            quarter(&mut x, 3, 4, 9, 14);
            round = round.wrapping_add(1);
        }
        let mut index = 0usize;
        while index < 16 {
            x[index] = x[index].wrapping_add(initial[index]);
            out[index * 4..index * 4 + 4].copy_from_slice(&x[index].to_le_bytes());
            index = index.wrapping_add(1);
        }
        self.input[12] = self.input[12].wrapping_add(1);
        if self.input[12] == 0 {
            self.input[13] = self.input[13].wrapping_add(1);
        }
    }
}

fn quarter(x: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    x[a] = x[a].wrapping_add(x[b]);
    x[d] = (x[d] ^ x[a]).rotate_left(16);
    x[c] = x[c].wrapping_add(x[d]);
    x[b] = (x[b] ^ x[c]).rotate_left(12);
    x[a] = x[a].wrapping_add(x[b]);
    x[d] = (x[d] ^ x[a]).rotate_left(8);
    x[c] = x[c].wrapping_add(x[d]);
    x[b] = (x[b] ^ x[c]).rotate_left(7);
}

#[cfg(test)]
#[path = "chacha_ut.rs"]
mod tests;

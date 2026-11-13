// SPDX-License-Identifier: MIT
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/crypt/crypt_sha256.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// (no per-file header upstream; covered by musl/COPYRIGHT)
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
// SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
//
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! SHA-256 and SHA-256-crypt (`$5$`).
//!
//! The donor is `musl:src/crypt/crypt_sha256.c` ("public domain sha256 crypt implementation",
//! MIT via `musl/COPYRIGHT`) because the pinned FreeBSD `lib/libcrypt` is absent. The format
//! constants are the ticket's: `ROUNDS_DEFAULT 5000`, `ROUNDS_MIN 1000`,
//! `ROUNDS_MAX 999999999` with clamping (the donor rejects over 9999999; see the report),
//! `SALT_MAX 16`. A `rounds=` prefix is echoed only when the setting carried one.

#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)] // fixed-size digest/state arrays and the donor round counters
use alloc::vec::Vec;

use crate::zero::scrub;

/// The donor's `KEY_MAX`.
pub const KEY_MAX: usize = 256;
/// `SALT_MAX`.
pub const SALT_MAX: usize = 16;
/// `ROUNDS_DEFAULT`.
pub const ROUNDS_DEFAULT: u32 = 5000;
/// `ROUNDS_MIN`.
pub const ROUNDS_MIN: u32 = 1000;
/// `ROUNDS_MAX` (the ticket's value; the donor's is 9999999).
pub const ROUNDS_MAX: u32 = 999_999_999;

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

const B64: &[u8] = b"./0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// The SHA-256 state of the donor.
#[derive(Clone)]
pub struct Sha256 {
    len: u64,
    h: [u32; 8],
    buf: [u8; 64],
}

impl Sha256 {
    /// `sha256_init`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            len: 0,
            h: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buf: [0; 64],
        }
    }

    #[allow(clippy::indexing_slicing, clippy::needless_range_loop)] // W is a fixed 64-word schedule
    fn processblock(&mut self, block: &[u8]) {
        let mut w = [0u32; 64];
        for i in 0..16usize {
            let base = i.wrapping_mul(4);
            w[i] = (u32::from(block.get(base).copied().unwrap_or(0)) << 24)
                | (u32::from(block.get(base.wrapping_add(1)).copied().unwrap_or(0)) << 16)
                | (u32::from(block.get(base.wrapping_add(2)).copied().unwrap_or(0)) << 8)
                | u32::from(block.get(base.wrapping_add(3)).copied().unwrap_or(0));
        }
        for i in 16..64usize {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = s1
                .wrapping_add(w[i - 7])
                .wrapping_add(s0)
                .wrapping_add(w[i - 16]);
        }
        let mut a = self.h[0];
        let mut b = self.h[1];
        let mut c = self.h[2];
        let mut d = self.h[3];
        let mut e = self.h[4];
        let mut f = self.h[5];
        let mut g = self.h[6];
        let mut h = self.h[7];
        for i in 0..64usize {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = g ^ (e & (f ^ g));
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) | (c & (a | b));
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        self.h[0] = self.h[0].wrapping_add(a);
        self.h[1] = self.h[1].wrapping_add(b);
        self.h[2] = self.h[2].wrapping_add(c);
        self.h[3] = self.h[3].wrapping_add(d);
        self.h[4] = self.h[4].wrapping_add(e);
        self.h[5] = self.h[5].wrapping_add(f);
        self.h[6] = self.h[6].wrapping_add(g);
        self.h[7] = self.h[7].wrapping_add(h);
    }

    #[allow(clippy::indexing_slicing)] // the padding writes are bounded by the 64-byte block
    fn pad(&mut self) {
        let mut r = usize::try_from(self.len & 63).unwrap_or(0);
        self.buf[r] = 0x80;
        r = r.wrapping_add(1);
        if r > 56 {
            for slot in self.buf.iter_mut().skip(r) {
                *slot = 0;
            }
            let block = self.buf;
            self.processblock(&block);
            r = 0;
        }
        for slot in self.buf.iter_mut().skip(r).take(56usize.saturating_sub(r)) {
            *slot = 0;
        }
        self.len = self.len.wrapping_mul(8);
        self.buf[56] = (self.len >> 56) as u8;
        self.buf[57] = (self.len >> 48) as u8;
        self.buf[58] = (self.len >> 40) as u8;
        self.buf[59] = (self.len >> 32) as u8;
        self.buf[60] = (self.len >> 24) as u8;
        self.buf[61] = (self.len >> 16) as u8;
        self.buf[62] = (self.len >> 8) as u8;
        self.buf[63] = self.len as u8;
        let block = self.buf;
        self.processblock(&block);
    }

    /// `sha256_update`.
    pub fn update(&mut self, mut data: &[u8]) {
        let r = usize::try_from(self.len & 63).unwrap_or(0);
        self.len = self.len.wrapping_add(data.len() as u64);
        if r != 0 {
            if data.len() < 64usize.wrapping_sub(r) {
                let end = r.wrapping_add(data.len());
                if let Some(slot) = self.buf.get_mut(r..end) {
                    slot.copy_from_slice(data);
                }
                return;
            }
            let take = 64usize.wrapping_sub(r);
            if let Some(slot) = self.buf.get_mut(r..64) {
                slot.copy_from_slice(&data[..take]);
            }
            let block = self.buf;
            self.processblock(&block);
            data = &data[take..];
        }
        while data.len() >= 64 {
            let block = data.get(..64).unwrap_or(&[]);
            self.processblock(block);
            data = data.get(64..).unwrap_or(&[]);
        }
        if let Some(slot) = self.buf.get_mut(..data.len()) {
            slot.copy_from_slice(data);
        }
    }

    /// `sha256_sum`.
    #[allow(clippy::indexing_slicing)] // eight 4-byte stores into a 32-byte digest
    pub fn sum(mut self) -> [u8; 32] {
        self.pad();
        let mut out = [0u8; 32];
        for index in 0..8usize {
            let value = self.h[index];
            let base = index.wrapping_mul(4);
            out[base] = (value >> 24) as u8;
            out[base.wrapping_add(1)] = (value >> 16) as u8;
            out[base.wrapping_add(2)] = (value >> 8) as u8;
            out[base.wrapping_add(3)] = value as u8;
        }
        out
    }
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

/// The SHA-256 digest of one buffer.
#[must_use]
pub fn digest(data: &[u8]) -> [u8; 32] {
    let mut context = Sha256::new();
    context.update(data);
    context.sum()
}

#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
fn to64(out: &mut Vec<u8>, mut value: u32, count: usize) {
    let mut remaining = count;
    while remaining > 0 {
        out.push(B64[(value % 64) as usize]);
        value /= 64;
        remaining = remaining.wrapping_sub(1);
    }
}

fn hashmd(context: &mut Sha256, mut count: usize, md: &[u8; 32]) {
    while count > 32 {
        context.update(md);
        count = count.wrapping_sub(32);
    }
    context.update(md.get(..count).unwrap_or(md));
}

fn parse_rounds(salt: &[u8]) -> Option<(u32, usize, bool)> {
    if !salt.starts_with(b"rounds=") {
        return Some((ROUNDS_DEFAULT, 0, false));
    }
    let digits = salt.get(7..)?;
    if digits.first().copied().is_none_or(|c| !c.is_ascii_digit()) {
        return None;
    }
    let mut value: u32 = 0;
    let mut index = 0usize;
    while index < digits.len() && digits[index].is_ascii_digit() {
        value = value
            .saturating_mul(10)
            .saturating_add(u32::from(digits[index].wrapping_sub(b'0')));
        index = index.wrapping_add(1);
    }
    if digits.get(index).copied() != Some(b'$') {
        return None;
    }
    let rounds = value.clamp(ROUNDS_MIN, ROUNDS_MAX);
    Some((rounds, 7usize.wrapping_add(index).wrapping_add(1), true))
}

/// `sha256crypt`: the `$5$` format.
///
/// Returns the NUL-less hash or `None` when the setting is malformed (the dispatch maps that to
/// `EINVAL`).
#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)] // fixed digest indexes and the donor round counter
pub fn sha256crypt(key: &[u8], setting: &[u8]) -> Option<Vec<u8>> {
    if key.len() > KEY_MAX {
        return None;
    }
    if !setting.starts_with(b"$5$") {
        return None;
    }
    let (rounds, salt_offset, custom) = parse_rounds(setting.get(3..)?)?;
    let salt_start = 3usize.wrapping_add(salt_offset);
    let mut salt_len = 0usize;
    while salt_len < SALT_MAX {
        match setting.get(salt_start.wrapping_add(salt_len)).copied() {
            None | Some(b'$') | Some(0) => break,
            Some(b'\n') | Some(b':') => return None,
            Some(_) => salt_len = salt_len.wrapping_add(1),
        }
    }
    let salt = setting.get(salt_start..salt_start.wrapping_add(salt_len))?;
    let klen = key.len();

    let mut context = Sha256::new();
    context.update(key);
    context.update(salt);
    context.update(key);
    let mut md = context.sum();

    let mut context = Sha256::new();
    context.update(key);
    context.update(salt);
    hashmd(&mut context, klen, &md);
    let mut bits = klen;
    while bits > 0 {
        if bits & 1 != 0 {
            context.update(&md);
        } else {
            context.update(key);
        }
        bits >>= 1;
    }
    md = context.sum();

    let mut context = Sha256::new();
    for _ in 0..klen {
        context.update(key);
    }
    let mut kmd = context.sum();

    let mut context = Sha256::new();
    let repeats = 16usize.wrapping_add(usize::from(md[0]));
    for _ in 0..repeats {
        context.update(salt);
    }
    let mut smd = context.sum();

    for index in 0..rounds {
        let mut context = Sha256::new();
        if index % 2 != 0 {
            hashmd(&mut context, klen, &kmd);
        } else {
            context.update(&md);
        }
        if index % 3 != 0 {
            context.update(&smd[..salt_len]);
        }
        if index % 7 != 0 {
            hashmd(&mut context, klen, &kmd);
        }
        if index % 2 != 0 {
            context.update(&md);
        } else {
            hashmd(&mut context, klen, &kmd);
        }
        md = context.sum();
    }

    let mut out = Vec::new();
    out.extend_from_slice(b"$5$");
    if custom {
        out.extend_from_slice(alloc::format!("rounds={rounds}$").as_bytes());
    }
    out.extend_from_slice(salt);
    out.push(b'$');
    const PERM: [[usize; 3]; 10] = [
        [0, 10, 20],
        [21, 1, 11],
        [12, 22, 2],
        [3, 13, 23],
        [24, 4, 14],
        [15, 25, 5],
        [6, 16, 26],
        [27, 7, 17],
        [18, 28, 8],
        [9, 19, 29],
    ];
    for perm in PERM {
        let value =
            (u32::from(md[perm[0]]) << 16) | (u32::from(md[perm[1]]) << 8) | u32::from(md[perm[2]]);
        to64(&mut out, value, 4);
    }
    to64(&mut out, (u32::from(md[31]) << 8) | u32::from(md[30]), 3);
    scrub(&mut md);
    scrub(&mut kmd);
    scrub(&mut smd);
    Some(out)
}

#[cfg(test)]
#[path = "sha256_ut.rs"]
mod tests;

// SPDX-License-Identifier: MIT
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/crypt/crypt_sha512.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! SHA-512 and SHA-512-crypt (`$6$`).
//!
//! The donor is `musl:src/crypt/crypt_sha512.c` ("public domain sha512 crypt implementation",
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

const K: [u64; 80] = [
    0x428a2f98d728ae22,
    0x7137449123ef65cd,
    0xb5c0fbcfec4d3b2f,
    0xe9b5dba58189dbbc,
    0x3956c25bf348b538,
    0x59f111f1b605d019,
    0x923f82a4af194f9b,
    0xab1c5ed5da6d8118,
    0xd807aa98a3030242,
    0x12835b0145706fbe,
    0x243185be4ee4b28c,
    0x550c7dc3d5ffb4e2,
    0x72be5d74f27b896f,
    0x80deb1fe3b1696b1,
    0x9bdc06a725c71235,
    0xc19bf174cf692694,
    0xe49b69c19ef14ad2,
    0xefbe4786384f25e3,
    0x0fc19dc68b8cd5b5,
    0x240ca1cc77ac9c65,
    0x2de92c6f592b0275,
    0x4a7484aa6ea6e483,
    0x5cb0a9dcbd41fbd4,
    0x76f988da831153b5,
    0x983e5152ee66dfab,
    0xa831c66d2db43210,
    0xb00327c898fb213f,
    0xbf597fc7beef0ee4,
    0xc6e00bf33da88fc2,
    0xd5a79147930aa725,
    0x06ca6351e003826f,
    0x142929670a0e6e70,
    0x27b70a8546d22ffc,
    0x2e1b21385c26c926,
    0x4d2c6dfc5ac42aed,
    0x53380d139d95b3df,
    0x650a73548baf63de,
    0x766a0abb3c77b2a8,
    0x81c2c92e47edaee6,
    0x92722c851482353b,
    0xa2bfe8a14cf10364,
    0xa81a664bbc423001,
    0xc24b8b70d0f89791,
    0xc76c51a30654be30,
    0xd192e819d6ef5218,
    0xd69906245565a910,
    0xf40e35855771202a,
    0x106aa07032bbd1b8,
    0x19a4c116b8d2d0c8,
    0x1e376c085141ab53,
    0x2748774cdf8eeb99,
    0x34b0bcb5e19b48a8,
    0x391c0cb3c5c95a63,
    0x4ed8aa4ae3418acb,
    0x5b9cca4f7763e373,
    0x682e6ff3d6b2b8a3,
    0x748f82ee5defb2fc,
    0x78a5636f43172f60,
    0x84c87814a1f0ab72,
    0x8cc702081a6439ec,
    0x90befffa23631e28,
    0xa4506cebde82bde9,
    0xbef9a3f7b2c67915,
    0xc67178f2e372532b,
    0xca273eceea26619c,
    0xd186b8c721c0c207,
    0xeada7dd6cde0eb1e,
    0xf57d4f7fee6ed178,
    0x06f067aa72176fba,
    0x0a637dc5a2c898a6,
    0x113f9804bef90dae,
    0x1b710b35131c471b,
    0x28db77f523047d84,
    0x32caab7b40c72493,
    0x3c9ebe0a15c9bebc,
    0x431d67c49c100d4c,
    0x4cc5d4becb3e42b6,
    0x597f299cfc657e2a,
    0x5fcb6fab3ad6faec,
    0x6c44198c4a475817,
];

const B64: &[u8] = b"./0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// The SHA-256 state of the donor.
#[derive(Clone)]
pub struct Sha512 {
    len: u64,
    h: [u64; 8],
    buf: [u8; 128],
}

impl Sha512 {
    /// `sha512_init`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            len: 0,
            h: [
                0x6a09e667f3bcc908,
                0xbb67ae8584caa73b,
                0x3c6ef372fe94f82b,
                0xa54ff53a5f1d36f1,
                0x510e527fade682d1,
                0x9b05688c2b3e6c1f,
                0x1f83d9abfb41bd6b,
                0x5be0cd19137e2179,
            ],
            buf: [0; 128],
        }
    }

    #[allow(clippy::indexing_slicing, clippy::needless_range_loop)] // W is a fixed 64-word schedule
    fn processblock(&mut self, block: &[u8]) {
        let mut w = [0u64; 80];
        for i in 0..16usize {
            let base = i.wrapping_mul(8);
            w[i] = (u64::from(block.get(base).copied().unwrap_or(0)) << 56)
                | (u64::from(block.get(base.wrapping_add(1)).copied().unwrap_or(0)) << 48)
                | (u64::from(block.get(base.wrapping_add(2)).copied().unwrap_or(0)) << 40)
                | (u64::from(block.get(base.wrapping_add(3)).copied().unwrap_or(0)) << 32)
                | (u64::from(block.get(base.wrapping_add(4)).copied().unwrap_or(0)) << 24)
                | (u64::from(block.get(base.wrapping_add(5)).copied().unwrap_or(0)) << 16)
                | (u64::from(block.get(base.wrapping_add(6)).copied().unwrap_or(0)) << 8)
                | u64::from(block.get(base.wrapping_add(7)).copied().unwrap_or(0));
        }
        for i in 16..80usize {
            let s0 = w[i - 15].rotate_right(1) ^ w[i - 15].rotate_right(8) ^ (w[i - 15] >> 7);
            let s1 = w[i - 2].rotate_right(19) ^ w[i - 2].rotate_right(61) ^ (w[i - 2] >> 6);
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
        for i in 0..80usize {
            let s1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
            let ch = g ^ (e & (f ^ g));
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
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
        let mut r = usize::try_from(self.len & 127).unwrap_or(0);
        self.buf[r] = 0x80;
        r = r.wrapping_add(1);
        if r > 112 {
            for slot in self.buf.iter_mut().skip(r) {
                *slot = 0;
            }
            let block = self.buf;
            self.processblock(&block);
            r = 0;
        }
        for slot in self.buf.iter_mut().skip(r).take(120usize.saturating_sub(r)) {
            *slot = 0;
        }
        self.len = self.len.wrapping_mul(8);
        self.buf[120] = (self.len >> 56) as u8;
        self.buf[121] = (self.len >> 48) as u8;
        self.buf[122] = (self.len >> 40) as u8;
        self.buf[123] = (self.len >> 32) as u8;
        self.buf[124] = (self.len >> 24) as u8;
        self.buf[125] = (self.len >> 16) as u8;
        self.buf[126] = (self.len >> 8) as u8;
        self.buf[127] = self.len as u8;
        let block = self.buf;
        self.processblock(&block);
    }

    /// `sha512_update`.
    pub fn update(&mut self, mut data: &[u8]) {
        let r = usize::try_from(self.len & 127).unwrap_or(0);
        self.len = self.len.wrapping_add(data.len() as u64);
        if r != 0 {
            if data.len() < 128usize.wrapping_sub(r) {
                let end = r.wrapping_add(data.len());
                if let Some(slot) = self.buf.get_mut(r..end) {
                    slot.copy_from_slice(data);
                }
                return;
            }
            let take = 128usize.wrapping_sub(r);
            if let Some(slot) = self.buf.get_mut(r..128) {
                slot.copy_from_slice(&data[..take]);
            }
            let block = self.buf;
            self.processblock(&block);
            data = &data[take..];
        }
        while data.len() >= 128 {
            let block = data.get(..128).unwrap_or(&[]);
            self.processblock(block);
            data = data.get(128..).unwrap_or(&[]);
        }
        if let Some(slot) = self.buf.get_mut(..data.len()) {
            slot.copy_from_slice(data);
        }
    }

    /// `sha512_sum`.
    #[allow(clippy::indexing_slicing)] // eight 4-byte stores into a 32-byte digest
    pub fn sum(mut self) -> [u8; 64] {
        self.pad();
        let mut out = [0u8; 64];
        for index in 0..8usize {
            let value = self.h[index];
            let base = index.wrapping_mul(8);
            out[base] = (value >> 56) as u8;
            out[base.wrapping_add(1)] = (value >> 48) as u8;
            out[base.wrapping_add(2)] = (value >> 40) as u8;
            out[base.wrapping_add(3)] = (value >> 32) as u8;
            out[base.wrapping_add(4)] = (value >> 24) as u8;
            out[base.wrapping_add(5)] = (value >> 16) as u8;
            out[base.wrapping_add(6)] = (value >> 8) as u8;
            out[base.wrapping_add(7)] = value as u8;
        }
        out
    }
}

impl Default for Sha512 {
    fn default() -> Self {
        Self::new()
    }
}

/// The SHA-256 digest of one buffer.
#[must_use]
pub fn digest(data: &[u8]) -> [u8; 64] {
    let mut context = Sha512::new();
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

fn hashmd(context: &mut Sha512, mut count: usize, md: &[u8; 64]) {
    while count > 64 {
        context.update(md);
        count = count.wrapping_sub(64);
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

/// `sha512crypt`: the `$6$` format.
///
/// Returns the NUL-less hash or `None` when the setting is malformed (the dispatch maps that to
/// `EINVAL`).
#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)] // fixed digest indexes and the donor round counter
pub fn sha512crypt(key: &[u8], setting: &[u8]) -> Option<Vec<u8>> {
    if key.len() > KEY_MAX {
        return None;
    }
    if !setting.starts_with(b"$6$") {
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

    let mut context = Sha512::new();
    context.update(key);
    context.update(salt);
    context.update(key);
    let mut md = context.sum();

    let mut context = Sha512::new();
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

    let mut context = Sha512::new();
    for _ in 0..klen {
        context.update(key);
    }
    let mut kmd = context.sum();

    let mut context = Sha512::new();
    let repeats = 16usize.wrapping_add(usize::from(md[0]));
    for _ in 0..repeats {
        context.update(salt);
    }
    let mut smd = context.sum();

    for index in 0..rounds {
        let mut context = Sha512::new();
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
    out.extend_from_slice(b"$6$");
    if custom {
        out.extend_from_slice(alloc::format!("rounds={rounds}$").as_bytes());
    }
    out.extend_from_slice(salt);
    out.push(b'$');
    const PERM: [[usize; 3]; 21] = [
        [0, 21, 42],
        [22, 43, 1],
        [44, 2, 23],
        [3, 24, 45],
        [25, 46, 4],
        [47, 5, 26],
        [6, 27, 48],
        [28, 49, 7],
        [50, 8, 29],
        [9, 30, 51],
        [31, 52, 10],
        [53, 11, 32],
        [12, 33, 54],
        [34, 55, 13],
        [56, 14, 35],
        [15, 36, 57],
        [37, 58, 16],
        [59, 17, 38],
        [18, 39, 60],
        [40, 61, 19],
        [62, 20, 41],
    ];
    for perm in PERM {
        let value =
            (u32::from(md[perm[0]]) << 16) | (u32::from(md[perm[1]]) << 8) | u32::from(md[perm[2]]);
        to64(&mut out, value, 4);
    }
    to64(&mut out, u32::from(md[63]), 2);
    scrub(&mut md);
    scrub(&mut kmd);
    scrub(&mut smd);
    Some(out)
}

#[cfg(test)]
#[path = "sha512_ut.rs"]
mod tests;

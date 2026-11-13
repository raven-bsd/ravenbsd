// SPDX-License-Identifier: MIT AND BSD-2-Clause
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2003 Poul-Henning Kamp
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/crypt/crypt_md5.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
// The md5crypt portion carries the donor's own notice:
//
// Copyright (c) 2003 Poul-Henning Kamp
// All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE AUTHOR AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.
//
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! MD5 and MD5-crypt (`$1$`), from `musl:src/crypt/crypt_md5.c`.
//!
//! The file is used because the pinned FreeBSD `lib/libcrypt` is absent from the donor
//! checkout; its MD5 core is the donor's "public domain md5 implementation based on rfc1321
//! and libtomcrypt" and the format code is Poul-Henning Kamp's BSD-2 md5crypt. Both are
//! allow-listed; the report records the substitution (RD-new). The key cap, the salt cap
//! (`SALT_MAX 8`), the donor base64 alphabet and the permutation table are the donor's.

#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)] // fixed-size digest/state arrays and the donor round counters
use alloc::vec::Vec;

use crate::zero::scrub;

const TAB: [u32; 64] = [
    0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
    0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
    0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
    0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
    0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
    0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
    0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
    0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
];

const KEY_MAX: usize = 30000;
const SALT_MAX: usize = 8;

const B64: &[u8] = b"./0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// The MD5 state of the donor (`struct md5`).
#[derive(Clone)]
pub struct Md5 {
    len: u64,
    h: [u32; 4],
    buf: [u8; 64],
}

#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
impl Md5 {
    /// `md5_init`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            len: 0,
            h: [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476],
            buf: [0; 64],
        }
    }

    fn processblock(&mut self, block: &[u8]) {
        let mut w = [0u32; 16];
        let mut index = 0usize;
        while index < 16 {
            let base = index.wrapping_mul(4);
            w[index] = u32::from(block.get(base).copied().unwrap_or(0))
                | u32::from(block.get(base.wrapping_add(1)).copied().unwrap_or(0)) << 8
                | u32::from(block.get(base.wrapping_add(2)).copied().unwrap_or(0)) << 16
                | u32::from(block.get(base.wrapping_add(3)).copied().unwrap_or(0)) << 24;
            index = index.wrapping_add(1);
        }
        let mut a = self.h[0];
        let mut b = self.h[1];
        let mut c = self.h[2];
        let mut d = self.h[3];
        for i in 0..64usize {
            let (f, g) = match i {
                0..=15 => ((b & c) | (!b & d), i),
                16..=31 => (
                    (d & b) | (!d & c),
                    (5usize.wrapping_mul(i).wrapping_add(1)) & 15,
                ),
                32..=47 => (b ^ c ^ d, (3usize.wrapping_mul(i).wrapping_add(5)) & 15),
                _ => (c ^ (b | !d), (7usize.wrapping_mul(i)) & 15),
            };
            let shift: u32 = match i {
                0..=15 => [7u32, 12, 17, 22][i & 3],
                16..=31 => [5u32, 9, 14, 20][i & 3],
                32..=47 => [4u32, 11, 16, 23][i & 3],
                _ => [6u32, 10, 15, 21][i & 3],
            };
            let sum = a
                .wrapping_add(f)
                .wrapping_add(w.get(g).copied().unwrap_or(0))
                .wrapping_add(TAB.get(i).copied().unwrap_or(0));
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(sum.rotate_left(shift));
        }
        self.h[0] = self.h[0].wrapping_add(a);
        self.h[1] = self.h[1].wrapping_add(b);
        self.h[2] = self.h[2].wrapping_add(c);
        self.h[3] = self.h[3].wrapping_add(d);
    }

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
        self.buf[56] = self.len as u8;
        self.buf[57] = (self.len >> 8) as u8;
        self.buf[58] = (self.len >> 16) as u8;
        self.buf[59] = (self.len >> 24) as u8;
        self.buf[60] = (self.len >> 32) as u8;
        self.buf[61] = (self.len >> 40) as u8;
        self.buf[62] = (self.len >> 48) as u8;
        self.buf[63] = (self.len >> 56) as u8;
        let block = self.buf;
        self.processblock(&block);
    }

    /// `md5_update`.
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

    /// `md5_sum`.
    pub fn sum(mut self) -> [u8; 16] {
        self.pad();
        let mut out = [0u8; 16];
        for index in 0..4usize {
            let value = self.h[index];
            let base = index.wrapping_mul(4);
            out[base] = value as u8;
            out[base.wrapping_add(1)] = (value >> 8) as u8;
            out[base.wrapping_add(2)] = (value >> 16) as u8;
            out[base.wrapping_add(3)] = (value >> 24) as u8;
        }
        out
    }
}

impl Default for Md5 {
    fn default() -> Self {
        Self::new()
    }
}

/// The MD5 digest of one buffer (the donor's `MD5Data` shape; used by the tests and bcrypt's
/// `$1$` path only).
#[must_use]
pub fn digest(data: &[u8]) -> [u8; 16] {
    let mut context = Md5::new();
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

/// `md5crypt`: the `$1$` format.
///
/// Returns the NUL-less hash, or `None` for a key over `KEY_MAX`, a setting that does not start
/// with `$1$`, or an empty setting (the dispatch rejects those with `EINVAL`).
#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)] // the donor permutation and round counter
pub fn md5crypt(key: &[u8], setting: &[u8]) -> Option<Vec<u8>> {
    if key.len() > KEY_MAX {
        return None;
    }
    if !setting.starts_with(b"$1$") {
        return None;
    }
    let salt_start = 3usize;
    let mut salt_len = 0usize;
    while salt_len < SALT_MAX {
        match setting.get(salt_start.wrapping_add(salt_len)).copied() {
            None | Some(b'$') | Some(0) => break,
            Some(_) => salt_len = salt_len.wrapping_add(1),
        }
    }
    let salt = setting.get(salt_start..salt_start.wrapping_add(salt_len))?;
    let klen = key.len();

    let mut context = Md5::new();
    context.update(key);
    context.update(salt);
    context.update(key);
    let mut md = context.sum();

    let mut context = Md5::new();
    context.update(key);
    context.update(setting.get(..salt_start.wrapping_add(salt_len))?);
    let mut remaining = klen;
    while remaining > md.len() {
        context.update(&md);
        remaining = remaining.wrapping_sub(md.len());
    }
    context.update(md.get(..remaining).unwrap_or(&md));
    let zero = [0u8; 1];
    let mut bits = klen;
    while bits > 0 {
        if bits & 1 != 0 {
            context.update(&zero);
        } else {
            // The donor reads one byte from the NUL-terminated key, i.e. a NUL for an empty key.
            context.update(&[key.first().copied().unwrap_or(0)]);
        }
        bits >>= 1;
    }
    md = context.sum();

    for index in 0..1000usize {
        let mut context = Md5::new();
        if index % 2 != 0 {
            context.update(key);
        } else {
            context.update(&md);
        }
        if index % 3 != 0 {
            context.update(salt);
        }
        if index % 7 != 0 {
            context.update(key);
        }
        if index % 2 != 0 {
            context.update(&md);
        } else {
            context.update(key);
        }
        md = context.sum();
    }

    let mut out = Vec::new();
    out.extend_from_slice(setting.get(..salt_start.wrapping_add(salt_len))?);
    out.push(b'$');
    const PERM: [[usize; 3]; 5] = [[0, 6, 12], [1, 7, 13], [2, 8, 14], [3, 9, 15], [4, 10, 5]];
    for perm in PERM {
        let value =
            (u32::from(md[perm[0]]) << 16) | (u32::from(md[perm[1]]) << 8) | u32::from(md[perm[2]]);
        to64(&mut out, value, 4);
    }
    to64(&mut out, u32::from(md[11]), 2);
    scrub(&mut md);
    Some(out)
}

#[cfg(test)]
#[path = "md5_ut.rs"]
mod tests;

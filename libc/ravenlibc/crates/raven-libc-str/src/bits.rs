// SPDX-License-Identifier: BSD-3-Clause
// Public domain. Written by Dale Rahn. (openbsd:lib/libc/string/ffs.c)
// Copyright (c) 1990, 1993
//	The Regents of the University of California.  All rights reserved.
// Copyright (c) 2023 The FreeBSD Foundation
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/string/ffs.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// and freebsd:lib/libc/string/fls.c, flsl.c, flsll.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb.
// ffs.c is public domain ("Public domain. Written by Dale Rahn."); the fls files carry the UCB
// BSD-3 text below.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
// 3. Neither the name of the University nor the names of its contributors
//    may be used to endorse or promote products derived from this software
//    without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.

//! Bit-index helpers: `ffs`, `ffsl`, `ffsll` and the `fls` family.

#![allow(clippy::arithmetic_side_effects)]

// ffs.c: the donor's 16-entry table. The unused slot 0 is -28 so that the all-zero input
// returns `28 + (-28) == 0` after the three halving steps (trap 12).
const FFS_TABLE: [i8; 16] = [-28, 1, 2, 1, 3, 1, 2, 1, 4, 1, 2, 1, 3, 1, 2, 1];

/// Index of the first (least significant) set bit, 1-based; 0 for 0.
#[must_use]
pub fn ffs(x: i32) -> i32 {
    let mut bit = 0i32;
    let mut value = x as u32;
    if value & 0xffff == 0 {
        bit += 16;
        value >>= 16;
    }
    if value & 0xff == 0 {
        bit += 8;
        value >>= 8;
    }
    if value & 0xf == 0 {
        bit += 4;
        value >>= 4;
    }
    let index = (value & 0xf) as usize;
    bit + i32::from(FFS_TABLE.get(index).copied().unwrap_or(0))
}

/// `ffs` for `long` (64-bit in the Raven ABI).
#[must_use]
pub fn ffsl(x: i64) -> i32 {
    if x == 0 {
        0
    } else {
        (x as u64).trailing_zeros() as i32 + 1
    }
}

/// `ffs` for `long long` (also 64-bit in the Raven ABI).
#[must_use]
pub fn ffsll(x: i64) -> i32 {
    ffsl(x)
}

/// Index of the last (most significant) set bit, 1-based; 0 for 0.
#[must_use]
pub fn fls(x: i32) -> i32 {
    if x == 0 {
        0
    } else {
        32 - (x as u32).leading_zeros() as i32
    }
}

/// `fls` for `long`.
#[must_use]
pub fn flsl(x: i64) -> i32 {
    if x == 0 {
        0
    } else {
        64 - (x as u64).leading_zeros() as i32
    }
}

/// `fls` for `long long`.
#[must_use]
pub fn flsll(x: i64) -> i32 {
    flsl(x)
}

#[cfg(test)]
#[path = "bits_ut.rs"]
mod tests;

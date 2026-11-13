// SPDX-License-Identifier: ISC
// Copyright (c) 2010 Damien Miller.  All rights reserved.
// Copyright (c) 2014 Google Inc.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/string/timingsafe_bcmp.c and timingsafe_memcmp.c
// @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tags: $OpenBSD: timingsafe_bcmp.c,v 1.3 ... $OpenBSD: timingsafe_memcmp.c,v 1.2 ...)
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

//! Constant-time comparisons (`timingsafe_bcmp`, `timingsafe_memcmp`).
//!
//! The engines read through the private `Load` trait so the unit test can count data accesses
//! with a counting implementation: the access pattern is data-independent by construction.
//!
//! RavenBSD: the bit arithmetic is the donor's fixed-width trick; no overflow is possible and the
//! lint allow records it.

#![allow(clippy::arithmetic_side_effects)]

/// A source of bytes by index; the production implementation reads a slice.
pub(crate) trait Load {
    /// Byte at `index`.
    fn get(&mut self, index: usize) -> u8;
}

struct SliceLoad<'a>(&'a [u8]);

impl Load for SliceLoad<'_> {
    #[inline]
    fn get(&mut self, index: usize) -> u8 {
        self.0.get(index).copied().unwrap_or(0)
    }
}

#[inline]
fn length(a: &[u8], b: &[u8]) -> usize {
    if a.len() < b.len() { a.len() } else { b.len() }
}

/// Constant-time `bcmp`: `1` when the buffers differ, `0` when they are equal.
///
/// Every byte of both inputs is read; no early exit. The result is normalised to 1/0 without a
/// conditional branch.
#[must_use]
pub fn timingsafe_bcmp(a: &[u8], b: &[u8]) -> i32 {
    let n = length(a, b);
    let mut left = SliceLoad(a);
    let mut right = SliceLoad(b);
    bcmp_with(&mut left, &mut right, n)
}

fn bcmp_with<L: Load, R: Load>(left: &mut L, right: &mut R, n: usize) -> i32 {
    let mut accumulator = 0i32;
    let mut index = 0usize;
    while index < n {
        accumulator |= i32::from(left.get(index)) ^ i32::from(right.get(index));
        index = index.wrapping_add(1);
    }
    // `accumulator` is 0..=255; branch-free normalisation to 1 iff it is non-zero.
    let value = core::hint::black_box(accumulator);
    (value | value.wrapping_neg()) >> 31 & 1
}

/// Constant-time `memcmp`: `-1`, `0` or `1` with a fixed trip count.
///
/// The donor's `(p1[i] - p2[i]) >> CHAR_BIT` is an **arithmetic** right shift of an `i32`; using
/// `u32` would silently break the comparison (A-str-10).
#[must_use]
pub fn timingsafe_memcmp(a: &[u8], b: &[u8]) -> i32 {
    let n = length(a, b);
    let mut left = SliceLoad(a);
    let mut right = SliceLoad(b);
    memcmp_with(&mut left, &mut right, n)
}

fn memcmp_with<L: Load, R: Load>(left: &mut L, right: &mut R, n: usize) -> i32 {
    let mut result = 0i32;
    let mut done = 0i32;
    let mut index = 0usize;
    while index < n {
        let first = i32::from(left.get(index));
        let second = i32::from(right.get(index));
        // `lt` is -1 if first < second, else 0; `gt` is -1 if first > second, else 0.
        let lt = first.wrapping_sub(second) >> 8;
        let gt = second.wrapping_sub(first) >> 8;
        let cmp = lt.wrapping_sub(gt);
        result |= cmp & !done;
        done |= lt | gt;
        index = index.wrapping_add(1);
    }
    result
}

#[cfg(test)]
#[path = "secure_ut.rs"]
mod tests;

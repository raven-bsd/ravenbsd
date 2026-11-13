// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1987, 1993
//	The Regents of the University of California.  All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/string/strcasecmp.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: strcasecmp.c,v 1.7 2015/08/31 02:53:57 guenther Exp $)
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

//! `strcasecmp` and `strncasecmp` over slices.

use super::byte_at;
use crate::ascii::LOWER;

#[inline]
fn lower(byte: u8) -> u8 {
    LOWER.get(usize::from(byte)).copied().unwrap_or(byte)
}

/// Case-insensitive `strcmp` under the C locale (ASCII `A` to `Z` only, bytes `>= 0x80` unchanged).
///
/// The result is the difference of the **lowercased** unsigned bytes (trap 5).
#[must_use]
pub fn strcasecmp(a: &[u8], b: &[u8]) -> i32 {
    let mut index = 0usize;
    loop {
        let left = byte_at(a, index);
        let right = byte_at(b, index);
        if lower(left) != lower(right) {
            return i32::from(lower(left)).wrapping_sub(i32::from(lower(right)));
        }
        if left == 0 {
            return 0;
        }
        index = index.wrapping_add(1);
    }
}

/// Case-insensitive `strcmp` over at most `n` bytes.
#[must_use]
pub fn strncasecmp(a: &[u8], b: &[u8], n: usize) -> i32 {
    let mut index = 0usize;
    while index < n {
        let left = byte_at(a, index);
        let right = byte_at(b, index);
        if lower(left) != lower(right) {
            return i32::from(lower(left)).wrapping_sub(i32::from(lower(right)));
        }
        if left == 0 {
            break;
        }
        index = index.wrapping_add(1);
    }
    0
}

#[cfg(test)]
#[path = "strcasecmp_ut.rs"]
mod tests;

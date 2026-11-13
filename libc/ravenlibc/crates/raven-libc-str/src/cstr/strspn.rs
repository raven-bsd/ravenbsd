// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1990 The Regents of the University of California.
// All rights reserved.
//
// This code is derived from software contributed to Berkeley by
// Chris Torek.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/string/strcspn.c, strspn.c and strpbrk.c
// @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tags: $OpenBSD: strcspn.c,v 1.6 ... $OpenBSD: strspn.c,v 1.6 ... $OpenBSD: strpbrk.c,v 1.6 ...)
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

//! The span family (`strspn`, `strcspn`, `strpbrk`) over slices.
//!
//! The donors are the simple O(n*m) loops; no 256-bit bitset is used (doc 01 choice). The slice
//! end acts as the terminating NUL, and `set` is the delimiter set without its NUL.

use super::byte_at;

/// Length of the prefix of `s` made only of bytes contained in `set`.
#[must_use]
pub fn strspn(s: &[u8], set: &[u8]) -> usize {
    let mut index = 0usize;
    while index < s.len() {
        let byte = byte_at(s, index);
        if byte == 0 || crate::mem::find(set, byte).is_none() {
            break;
        }
        index = index.wrapping_add(1);
    }
    index
}

/// Length of the prefix of `s` that contains no byte of `set` (`strcspn(s, "")` is `strlen(s)`).
#[must_use]
pub fn strcspn(s: &[u8], set: &[u8]) -> usize {
    let mut index = 0usize;
    while index < s.len() {
        let byte = byte_at(s, index);
        if byte == 0 || crate::mem::find(set, byte).is_some() {
            break;
        }
        index = index.wrapping_add(1);
    }
    index
}

/// Index of the first byte of `s` that is in `set`, or `None` (`strpbrk(s, "")` is `NULL`).
#[must_use]
pub fn strpbrk(s: &[u8], set: &[u8]) -> Option<usize> {
    let mut index = 0usize;
    while index < s.len() {
        let byte = byte_at(s, index);
        if byte == 0 {
            return None;
        }
        if crate::mem::find(set, byte).is_some() {
            return Some(index);
        }
        index = index.wrapping_add(1);
    }
    None
}

#[cfg(test)]
#[path = "strspn_ut.rs"]
mod tests;

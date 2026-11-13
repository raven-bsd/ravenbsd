// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1990, 1993
//	The Regents of the University of California.  All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/string/strcasestr.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tags: $OpenBSD: strcasestr.c,v 1.4 2015/08/31 02:53:57 guenther Exp $
//  $NetBSD: strcasestr.c,v 1.2 2005/02/09 21:35:47 kleink Exp $)
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

//! Case-insensitive `strstr` over slices.

use super::byte_at;
use crate::ascii::LOWER;

#[inline]
fn lower(byte: u8) -> u8 {
    LOWER.get(usize::from(byte)).copied().unwrap_or(byte)
}

/// Index of the first case-insensitive occurrence of `needle` in `haystack`.
///
/// An empty needle returns the haystack start (offset 0). Only ASCII `A` to `Z` folds.
#[must_use]
pub fn strcasestr(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    let first = lower(byte_at(needle, 0));
    let mut index = 0usize;
    loop {
        let byte = byte_at(haystack, index);
        if byte == 0 {
            return None;
        }
        index = index.wrapping_add(1);
        if lower(byte) != first {
            continue;
        }
        let mut matched = 1usize;
        while matched < needle.len()
            && lower(byte_at(
                haystack,
                index.wrapping_sub(1).wrapping_add(matched),
            )) == lower(byte_at(needle, matched))
        {
            matched = matched.wrapping_add(1);
        }
        if matched == needle.len() {
            return Some(index.wrapping_sub(1));
        }
    }
}

#[cfg(test)]
#[path = "strcasestr_ut.rs"]
mod tests;

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2001 Mike Barcroft <mike@FreeBSD.org>
// Copyright (c) 1990, 1993
//	The Regents of the University of California.  All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:lib/libc/string/strnstr.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// (file revision tag: $FreeBSD$ line absent; SPDX-License-Identifier: BSD-3-Clause in the header)
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

//! `strnstr` over slices: a NUL-terminated search bounded by `len` bytes.

use super::byte_at;

/// Index of the first occurrence of `needle` in `haystack[..len]`, stopping at a NUL.
///
/// The caller guarantees that `haystack` is readable for `len` bytes; the slice passed here has
/// exactly that length. An empty needle matches at offset 0.
#[must_use]
pub fn strnstr(haystack: &[u8], needle: &[u8], len: usize) -> Option<usize> {
    let limit = if haystack.len() < len {
        haystack.len()
    } else {
        len
    };
    if needle.is_empty() {
        return Some(0);
    }
    let first = byte_at(needle, 0);
    let mut index = 0usize;
    loop {
        if index >= limit {
            return None;
        }
        let byte = byte_at(haystack, index);
        index = index.wrapping_add(1);
        if byte == 0 {
            return None;
        }
        if byte != first {
            continue;
        }
        let remaining = limit.wrapping_sub(index);
        if needle.len().wrapping_sub(1) > remaining {
            return None;
        }
        let mut matched = 1usize;
        while matched < needle.len()
            && byte_at(haystack, index.wrapping_sub(1).wrapping_add(matched))
                == byte_at(needle, matched)
        {
            matched = matched.wrapping_add(1);
        }
        if matched == needle.len() {
            return Some(index.wrapping_sub(1));
        }
    }
}

#[cfg(test)]
#[path = "strnstr_ut.rs"]
mod tests;

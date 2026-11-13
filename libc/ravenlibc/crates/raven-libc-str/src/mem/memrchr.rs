// SPDX-License-Identifier: ISC
// Copyright (c) 2007 Todd C. Miller <millert@openbsd.org>
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/string/memrchr.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: memrchr.c,v 1.4 2019/01/25 00:19:25 millert Exp $)
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

//! Reverse `memchr` (`memrchr`): the last occurrence of a byte in a slice.

/// Index of the last byte of `h` equal to `c`, or `None`.
///
/// The C walks `cp = s + n` and pre-decrements; an empty buffer returns `NULL` without any read.
#[must_use]
pub fn rfind(h: &[u8], c: u8) -> Option<usize> {
    let mut index = h.len();
    while index != 0 {
        index = index.wrapping_sub(1);
        let Some(&byte) = h.get(index) else {
            break;
        };
        if byte == c {
            return Some(index);
        }
    }
    None
}

#[cfg(test)]
#[path = "memrchr_ut.rs"]
mod tests;

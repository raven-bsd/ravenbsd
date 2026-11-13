// SPDX-License-Identifier: ISC
// Copyright (c) 1998, 2015 Todd C. Miller <millert@openbsd.org>
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/string/strlcat.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: strlcat.c,v 1.19 2019/01/25 00:19:25 millert Exp $)
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

//! `strlcat` on slices.

use super::byte_at;

/// Append the C string `src` to the string in `dst` (`dst.len()` is `dsize`).
///
/// Returns `min(strlen(dst), dsize) + strlen(src)`. When no NUL is found within `dsize`, `dst`
/// is left untouched (the C `if (n-- == 0) return (dlen + strlen(src))` path).
#[must_use]
pub fn strlcat(dst: &mut [u8], src: &[u8]) -> usize {
    let dsize = dst.len();
    let mut n = dsize;
    let mut dlen = 0usize;

    // C: while (n-- != 0 && *dst != '\0') dst++;
    loop {
        let more = n != 0;
        n = n.wrapping_sub(1);
        if !more {
            break;
        }
        let byte = dst.get(dlen).copied().unwrap_or(0);
        if byte == 0 {
            break;
        }
        dlen = dlen.wrapping_add(1);
    }
    n = dsize.wrapping_sub(dlen);

    // C: if (n-- == 0) return (dlen + strlen(src));
    let space = n;
    n = n.wrapping_sub(1);
    if space == 0 {
        return dlen.wrapping_add(src.len());
    }

    // C: while (*src != '\0') { if (n != 0) { *dst++ = *src; n--; } src++; }
    let mut si = 0usize;
    let mut di = dlen;
    while si < src.len() {
        let byte = byte_at(src, si);
        if n != 0 {
            if let Some(slot) = dst.get_mut(di) {
                *slot = byte;
            }
            di = di.wrapping_add(1);
            n = n.wrapping_sub(1);
        }
        si = si.wrapping_add(1);
    }

    // C: *dst = '\0';
    if let Some(slot) = dst.get_mut(di) {
        *slot = 0;
    }

    dlen.wrapping_add(src.len())
}

#[cfg(test)]
#[path = "strlcat_ut.rs"]
mod tests;

// SPDX-License-Identifier: ISC
// Copyright (c) 1998, 2015 Todd C. Miller <millert@openbsd.org>
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/string/strlcpy.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: strlcpy.c,v 1.16 2019/01/25 00:19:25 millert Exp $)
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

//! `strlcpy` on slices.

use super::byte_at;

/// Copy the C string `src` into `dst` (`dst.len()` is `dsize`).
///
/// At most `dsize - 1` bytes are copied and the result is always NUL terminated unless
/// `dsize == 0`. Returns `strlen(src)`; a result `>= dst.len()` means truncation.
#[must_use]
pub fn strlcpy(dst: &mut [u8], src: &[u8]) -> usize {
    let dsize = dst.len();
    let mut nleft = dsize;
    let mut di = 0usize;
    let mut si = 0usize;

    // C: if (nleft != 0) while (--nleft != 0) { if ((*dst++ = *src++) == '\0') break; }
    if nleft != 0 {
        loop {
            nleft = nleft.wrapping_sub(1);
            if nleft == 0 {
                break;
            }
            let byte = byte_at(src, si);
            si = si.wrapping_add(1);
            if let Some(slot) = dst.get_mut(di) {
                *slot = byte;
            }
            di = di.wrapping_add(1);
            if byte == 0 {
                break;
            }
        }
    }

    // C: if (nleft == 0) { if (dsize != 0) *dst = '\0'; while (*src++); }
    if nleft == 0 {
        if dsize != 0
            && let Some(slot) = dst.get_mut(di)
        {
            *slot = 0;
        }
        while byte_at(src, si) != 0 {
            si = si.wrapping_add(1);
        }
        si = si.wrapping_add(1);
    }

    si.wrapping_sub(1)
}

#[cfg(test)]
#[path = "strlcpy_ut.rs"]
mod tests;

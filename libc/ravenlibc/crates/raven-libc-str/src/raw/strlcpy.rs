// SPDX-License-Identifier: ISC
// Copyright (c) 1998, 2015 Todd C. Miller <millert@openbsd.org>
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/string/strlcpy.c and strlcat.c
// @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tags: $OpenBSD: strlcpy.c,v 1.16 ... $OpenBSD: strlcat.c,v 1.19 ...)
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

//! `strlcpy` and `strlcat` pointer entry points.

/// Copy the C string `src` into `dst` of size `dsize`; returns `strlen(src)`.
///
/// # Safety
///
/// `dst` is writable for `dsize` bytes (`dsize == 0` allows a null `dst`), `src` is NUL
/// terminated, and the ranges do not overlap.
pub unsafe fn strlcpy(dst: *mut u8, src: *const u8, dsize: usize) -> usize {
    // SAFETY: `strlen` stays inside `src`; the engine writes at most `dsize` bytes of `dst`.
    unsafe {
        let length = super::strlen::strlen(src);
        crate::cstr::strlcpy(
            super::common::bytes_mut(dst, dsize),
            super::common::bytes(src, length),
        )
    }
}

/// Append the C string `src` to `dst` of size `dsize`; returns `min(strlen(dst), dsize) +
/// strlen(src)`.
///
/// # Safety
///
/// `dst` is writable for `dsize` bytes and holds a NUL-terminated string when `dsize != 0`;
/// `src` is NUL terminated; the ranges do not overlap.
pub unsafe fn strlcat(dst: *mut u8, src: *const u8, dsize: usize) -> usize {
    // SAFETY: `strlen(src)` stays inside `src`; the engine scans at most `dsize` bytes of `dst`.
    unsafe {
        let length = super::strlen::strlen(src);
        crate::cstr::strlcat(
            super::common::bytes_mut(dst, dsize),
            super::common::bytes(src, length),
        )
    }
}

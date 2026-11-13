// SPDX-License-Identifier: ISC
// Copyright (c) 2014 Ted Unangst <tedu@openbsd.org>
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/string/swab.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: swab.c,v 1.9 2014/12/11 23:05:38 tedu Exp $)
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

//! `swab` pointer entry point.

/// Swap adjacent byte pairs; `len <= 1` does nothing and an odd last byte is not copied.
///
/// # Safety
///
/// `from` is readable for `max(len, 0)` bytes and `to` writable for the same number; the ranges
/// do not overlap.
pub unsafe fn swab(from: *const u8, to: *mut u8, len: isize) {
    let mut left = len;
    let mut source = from;
    let mut dest = to;
    // SAFETY: the caller's `swab` contract; the loop reads and writes two bytes at a time while
    // more than one byte remains, so the odd tail is neither read nor written.
    unsafe {
        while left > 1 {
            let first = *source;
            let second = *source.add(1);
            *dest = second;
            *dest.add(1) = first;
            source = source.add(2);
            dest = dest.add(2);
            left = left.wrapping_sub(2);
        }
    }
}

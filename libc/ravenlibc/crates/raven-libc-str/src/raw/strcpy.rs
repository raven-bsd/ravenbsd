// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1988 The Regents of the University of California.
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/string/strcpy.c, strcat.c and strncat.c
// @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tags: $OpenBSD: strcpy.c,v 1.10 ... $OpenBSD: strcat.c,v 1.10 ...
//  $OpenBSD: strncat.c,v 1.6 ...)
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

//! `strcpy`, `strcat` and `strncat` pointer entry points.

/// Copy the C string at `s` to `d`; returns `d`.
///
/// # Safety
///
/// `d` is writable for the length of `s` plus one; `s` is NUL terminated; no overlap.
pub unsafe fn strcpy(d: *mut u8, s: *const u8) -> *mut u8 {
    let mut source = s;
    let mut dest = d;
    // SAFETY: the caller's contract; the loop writes one byte per byte read and stops after the
    // NUL, which the destination has room for.
    unsafe {
        loop {
            let byte = *source;
            *dest = byte;
            if byte == 0 {
                return d;
            }
            source = source.add(1);
            dest = dest.add(1);
        }
    }
}

/// Append the C string at `s` to the string at `d`; returns `d`.
///
/// # Safety
///
/// `d` is a NUL-terminated string with room for `s` and its NUL; `s` is NUL terminated; no
/// overlap.
pub unsafe fn strcat(d: *mut u8, s: *const u8) -> *mut u8 {
    // SAFETY: `d` is NUL terminated, so `strlen` stays in the object; `strcpy` then writes into
    // the caller-provided room.
    unsafe {
        let length = super::strlen::strlen(d);
        super::strcpy::strcpy(d.add(length), s);
    }
    d
}

/// Append at most `n` bytes of the C string at `s` to `d`; always NUL terminates.
///
/// # Safety
///
/// `d` is a NUL-terminated string with room for `min(strlen(s), n) + 1` more bytes; `s` is
/// readable for `n` bytes; no overlap.
pub unsafe fn strncat(d: *mut u8, s: *const u8, n: usize) -> *mut u8 {
    // SAFETY: `strlen`/`strnlen` stay inside their objects and the append writes `count + 1`
    // bytes inside the room the caller promised.
    unsafe {
        let dst_len = super::strlen::strlen(d);
        let src_len = super::strnlen::strnlen(s, n);
        let source = super::common::bytes(s, src_len);
        let count = crate::cstr::strncat_len(dst_len, source, n);
        let mut index = 0usize;
        while index < count {
            *d.add(dst_len).add(index) = source.get(index).copied().unwrap_or(0);
            index = index.wrapping_add(1);
        }
        *d.add(dst_len).add(count) = 0;
    }
    d
}

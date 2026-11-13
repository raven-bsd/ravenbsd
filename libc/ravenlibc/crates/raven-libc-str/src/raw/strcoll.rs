// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1990 The Regents of the University of California.
// All rights reserved.
//
// This code is derived from software contributed to Berkeley by
// Chris Torek.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/string/strcoll.c and strxfrm.c
// @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tags: $OpenBSD: strcoll.c,v 1.6 ... $OpenBSD: strxfrm.c,v 1.7 ...)
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

//! `strcoll` and `strxfrm` pointer entry points.

/// `strcoll` under the only implemented collation (UTF-8 byte order): identical to `strcmp`.
///
/// # Safety
///
/// Both pointers are NUL-terminated strings.
pub unsafe fn strcoll(a: *const u8, b: *const u8) -> i32 {
    // SAFETY: forwards the caller's `strcmp` contract.
    unsafe { super::strcmp::strcmp(a, b) }
}

/// Copy `src` like `strlcpy` and return `strlen(src)`; `n == 0` allows a null `dst`.
///
/// # Safety
///
/// When `n != 0`, `dst` is writable for `n` bytes; `src` is NUL terminated; no overlap.
pub unsafe fn strxfrm(dst: *mut u8, src: *const u8, n: usize) -> usize {
    // SAFETY: `strlen` stays inside `src`; `strlcpy` writes at most `n` bytes.
    unsafe {
        let length = super::strlen::strlen(src);
        if n == 0 {
            return length;
        }
        crate::cstr::strlcpy(
            super::common::bytes_mut(dst, n),
            super::common::bytes(src, length),
        )
    }
}

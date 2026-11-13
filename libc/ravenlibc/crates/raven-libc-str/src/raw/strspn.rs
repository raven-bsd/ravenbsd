// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1990 The Regents of the University of California.
// All rights reserved.
//
// This code is derived from software contributed to Berkeley by
// Chris Torek.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/string/strspn.c, strcspn.c and strpbrk.c
// @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tags: $OpenBSD: strspn.c,v 1.6 ... $OpenBSD: strcspn.c,v 1.6 ...
//  $OpenBSD: strpbrk.c,v 1.6 ...)
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

//! `strspn`, `strcspn` and `strpbrk` pointer entry points.

/// Length of the prefix of `s` made only of bytes in `set`.
///
/// # Safety
///
/// Both pointers are NUL-terminated strings.
pub unsafe fn strspn(s: *const u8, set: *const u8) -> usize {
    // SAFETY: both strings are read to their NUL by `strlen`; the engine then reads those bytes.
    unsafe {
        let length = super::strlen::strlen(s);
        let set_length = super::strlen::strlen(set);
        crate::cstr::strspn(
            super::common::bytes(s, length),
            super::common::bytes(set, set_length),
        )
    }
}

/// Length of the prefix of `s` without any byte of `set`.
///
/// # Safety
///
/// Both pointers are NUL-terminated strings.
pub unsafe fn strcspn(s: *const u8, set: *const u8) -> usize {
    // SAFETY: both strings are read to their NUL by `strlen`.
    unsafe {
        let length = super::strlen::strlen(s);
        let set_length = super::strlen::strlen(set);
        crate::cstr::strcspn(
            super::common::bytes(s, length),
            super::common::bytes(set, set_length),
        )
    }
}

/// Pointer to the first byte of `s` that is in `set`, or null.
///
/// # Safety
///
/// Both pointers are NUL-terminated strings.
pub unsafe fn strpbrk(s: *const u8, set: *const u8) -> *mut u8 {
    // SAFETY: both strings are read to their NUL; the returned offset is inside `s`.
    unsafe {
        let length = super::strlen::strlen(s);
        let set_length = super::strlen::strlen(set);
        match crate::cstr::strpbrk(
            super::common::bytes(s, length),
            super::common::bytes(set, set_length),
        ) {
            Some(offset) => s.add(offset) as *mut u8,
            None => core::ptr::null_mut(),
        }
    }
}

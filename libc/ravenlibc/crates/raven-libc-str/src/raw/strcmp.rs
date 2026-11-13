// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1990 The Regents of the University of California.
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/string/strcmp.c and strncmp.c
// @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tags: $OpenBSD: strcmp.c,v 1.9 ... $OpenBSD: strncmp.c,v 1.9 ...)
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

//! `strcmp` and `strncmp` pointer entry points.

/// Compare two C strings; the result is the unsigned-char difference of the first difference.
///
/// # Safety
///
/// Both pointers are NUL-terminated strings.
pub unsafe fn strcmp(a: *const u8, b: *const u8) -> i32 {
    // SAFETY: both `strlen` calls stay inside the caller's strings; the engine reads exactly
    // those byte ranges.
    unsafe {
        let left_len = super::strlen::strlen(a);
        let right_len = super::strlen::strlen(b);
        crate::cstr::strcmp(
            super::common::bytes(a, left_len),
            super::common::bytes(b, right_len),
        )
    }
}

/// Compare at most `n` bytes of two C strings; stops at a NUL.
///
/// # Safety
///
/// Both pointers are readable for `n` bytes.
pub unsafe fn strncmp(a: *const u8, b: *const u8, n: usize) -> i32 {
    // SAFETY: `strnlen` reads at most `n` bytes and never past a NUL; the engine reads the
    // resulting bounded slices.
    unsafe {
        let left_len = super::strnlen::strnlen(a, n);
        let right_len = super::strnlen::strnlen(b, n);
        crate::cstr::strncmp(
            super::common::bytes(a, left_len),
            super::common::bytes(b, right_len),
            n,
        )
    }
}

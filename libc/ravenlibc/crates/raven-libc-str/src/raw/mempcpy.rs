// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2021 The FreeBSD Foundation
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:lib/libc/string/mempcpy.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// (no $FreeBSD$ tag; SPDX-License-Identifier: BSD-2-Clause in the header)
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE AUTHOR AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.

//! `mempcpy` pointer entry point.

/// `memcpy` returning `dst + n` instead of `dst`.
///
/// # Safety
///
/// `dst` is writable for `n` bytes, `src` is readable for `n` bytes, and the ranges do not
/// overlap.
pub unsafe fn mempcpy(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    // SAFETY: the caller's contract is exactly `memcpy`'s.
    unsafe {
        super::mem::memcpy(dst, src, n);
        dst.add(n)
    }
}

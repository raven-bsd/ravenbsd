// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1990, 1993
//	The Regents of the University of California.  All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/string/memccpy.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: memccpy.c,v 1.7 2015/08/31 02:53:57 guenther Exp $)
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

//! `memccpy` pointer entry point.

/// Copy bytes from `f` to `t`, stopping after the first byte equal to `c`; returns the position
/// after it, or null when `c` does not occur in `n` bytes.
///
/// # Safety
///
/// `t` is writable for `n` bytes and `f` is readable for `n` bytes; the ranges do not overlap.
pub unsafe fn memccpy(t: *mut u8, f: *const u8, c: i32, n: usize) -> *mut u8 {
    // SAFETY: both ranges are valid for `n` bytes and do not overlap, so the engine's slice
    // view is sound for the duration of the call.
    let (dst, src) = unsafe { (super::common::bytes_mut(t, n), super::common::bytes(f, n)) };
    match crate::mem::ccopy(dst, src, c as u8) {
        Some(offset) => {
            // SAFETY: `offset <= n` because the engine copied at most `n` bytes.
            unsafe { t.add(offset) }
        }
        None => core::ptr::null_mut(),
    }
}

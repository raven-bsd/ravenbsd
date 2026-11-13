// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1990, 1993
//	The Regents of the University of California.  All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/string/strsep.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: strsep.c,v 1.8 2015/08/31 02:53:57 guenther Exp $)
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

//! `strsep` pointer entry point.

/// Next field of `*stringp`, writing NULs into the buffer; `*stringp` becomes null after the
/// last field.
///
/// # Safety
///
/// `stringp` is a valid `char **`; when `*stringp` is non-null it is a NUL-terminated writable
/// string; `delim` is a NUL-terminated string.
pub unsafe fn strsep(stringp: *mut *mut u8, delim: *const u8) -> *mut u8 {
    // SAFETY: the caller owns `stringp` and promises the string/delimiter contracts.
    unsafe {
        let start = *stringp;
        if start.is_null() {
            return core::ptr::null_mut();
        }
        let delim_len = super::strlen::strlen(delim);
        let delim_slice = super::common::bytes(delim, delim_len);
        let length = super::strlen::strlen(start);
        let buffer = super::common::bytes_mut(start, length.wrapping_add(1));
        let mut position = Some(0usize);
        let token = crate::cstr::strsep(buffer, &mut position, delim_slice);
        *stringp = match position {
            Some(offset) => start.add(offset),
            None => core::ptr::null_mut(),
        };
        match token {
            Some((offset, _)) => start.add(offset),
            None => core::ptr::null_mut(),
        }
    }
}

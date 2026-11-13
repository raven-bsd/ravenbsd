// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1988 Regents of the University of California.
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/string/strtok.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (no $OpenBSD$ tag in the donor; the licence text is the UCB BSD-3 below)
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

//! `strtok_r` and `strtok` pointer entry points.
//!
//! RavenBSD: the donor's function-local `static char *last` is deleted; `strtok` receives the
//! per-thread cursor slot from the caller (RD-14). There is no global state in this crate.

/// Next token of `s` (or of `*last` when `s` is null), writing NULs into the buffer.
///
/// # Safety
///
/// When `s` is non-null it is a NUL-terminated writable string; otherwise `*last` is either null
/// or the continuation pointer produced by an earlier call on the same buffer. `last` is a valid
/// `char **`.
pub unsafe fn strtok_r(s: *mut u8, delim: *const u8, last: *mut *mut u8) -> *mut u8 {
    // SAFETY: the caller owns `last`; reading it is the C `*last` access.
    let mut start = unsafe { *last };
    if !s.is_null() {
        start = s;
    }
    if start.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: `delim` is a NUL-terminated string, so `strlen` stays inside it.
    let delim_len = unsafe { super::strlen::strlen(delim) };
    // SAFETY: `delim` is readable for `delim_len` bytes by the `strlen` result above.
    let delim_slice = unsafe { super::common::bytes(delim, delim_len) };
    // SAFETY: `start` is a NUL-terminated string; the engine only reads and writes inside the
    // string plus its terminator.
    unsafe {
        let length = super::strlen::strlen(start);
        let buffer = super::common::bytes_mut(start, length.wrapping_add(1));
        let mut cursor = Some(0usize);
        let token = crate::cstr::strtok(buffer, &mut cursor, delim_slice);
        *last = match cursor {
            Some(offset) => start.add(offset),
            None => core::ptr::null_mut(),
        };
        match token {
            Some((offset, _)) => start.add(offset),
            None => core::ptr::null_mut(),
        }
    }
}

/// `strtok` with a caller-provided per-thread cursor slot.
///
/// # Safety
///
/// `cursor` is a valid `char **` slot; `s` is a NUL-terminated writable string when non-null;
/// `delim` is a NUL-terminated string.
pub unsafe fn strtok(cursor: *mut *mut u8, s: *mut u8, delim: *const u8) -> *mut u8 {
    // SAFETY: forwards the caller's contract to `strtok_r`.
    unsafe { strtok_r(s, delim, cursor) }
}

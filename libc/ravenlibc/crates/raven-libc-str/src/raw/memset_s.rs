// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2017 Juniper Networks.  All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:lib/libc/string/memset_s.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// (the donor carries the BSD-2 text below without an SPDX line; class read as BSD-2-Clause,
// lines 1-30). Ring 2 by CR-22 D-2 (H-21): the Annex K constraint handler is not shipped, the
// error code is the whole contract (RD-12).
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

//! C11 Annex K `memset_s` (the one Annex K function that ships, CR-22 D-2).

/// `RSIZE_MAX` of the C standard: `SIZE_MAX >> 1`.
pub const RSIZE_MAX: usize = usize::MAX >> 1;

/// Fill `n` bytes at `s` with `c`, never an optimisable dead store.
///
/// Returns `0` on success and `EINVAL` when `s` is null, `smax`/`n` exceed [`RSIZE_MAX`], or
/// `n > smax` (in which case `smax` bytes are still filled). Never sets `errno` and never calls a
/// constraint handler.
///
/// # Safety
///
/// When the call is valid, `s` is writable for `min(n, smax)` bytes.
pub unsafe fn memset_s(s: *mut u8, smax: usize, c: i32, n: usize) -> i32 {
    let invalid = raven_libc_abi::errno::EINVAL;
    if s.is_null() || smax > RSIZE_MAX || n > RSIZE_MAX {
        return invalid;
    }
    let limit = if n < smax { n } else { smax };
    let value = c as u8;
    // SAFETY: the caller's valid-call contract gives `limit <= smax` writable bytes at `s`; the
    // stores are volatile so the fill cannot be removed.
    unsafe {
        let mut index = 0usize;
        while index < limit {
            core::ptr::write_volatile(s.add(index), value);
            index = index.wrapping_add(1);
        }
        core::arch::asm!("/* {s} */", s = in(reg) s, options(nostack, preserves_flags));
    }
    if n > smax { invalid } else { 0 }
}

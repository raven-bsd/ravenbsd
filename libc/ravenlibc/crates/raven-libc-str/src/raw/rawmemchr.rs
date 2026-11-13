// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `rawmemchr` (GNU): `memchr` without a bound.
//!
//! CLEANROOM: no donor file is copied (musl has no `rawmemchr`). The contract was read from the
//! glibc `string/rawmemchr.c` documentation only; the loop is original.

/// First occurrence of byte `c`, without a length limit.
///
/// # Safety
///
/// `c` occurs somewhere in the object at `s`; the caller guarantees the byte is present (the C
/// function is undefined otherwise).
pub unsafe fn rawmemchr(s: *const u8, c: i32) -> *mut u8 {
    let byte = c as u8;
    let mut p = s;
    // SAFETY: the caller guarantees the searched byte exists, so the loop terminates inside the
    // object before any unmapped memory is read.
    unsafe {
        while *p != byte {
            p = p.add(1);
        }
    }
    p as *mut u8
}

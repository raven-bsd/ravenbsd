// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `rawmemchr(3)`.
use core::ffi::{c_int, c_void};

/// The first `c`, with no length bound (GNU).
///
/// # Safety
/// `s` points to a readable region that contains `c` before any unreadable byte.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rawmemchr(s: *const c_void, c: c_int) -> *mut c_void {
    // SAFETY: the caller's contract is the rawmemchr(3) contract.
    unsafe { raven_libc_str::raw::rawmemchr(s.cast(), c).cast() }
}

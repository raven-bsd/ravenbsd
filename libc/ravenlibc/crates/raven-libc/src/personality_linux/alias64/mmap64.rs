// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `mmap64`: the 64-bit `off_t` spelling of `mmap` (ticket C66 5.F).
use core::ffi::{c_int, c_void};

/// Forward to `mmap`.
///
/// # Safety
/// The caller's `mmap` contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmap64(
    addr: *mut c_void,
    len: usize,
    protection: c_int,
    flags: c_int,
    fd: c_int,
    offset: i64,
) -> *mut c_void {
    // SAFETY: forwarded from this function's contract.
    unsafe { crate::platform::mmap::mmap(addr, len, protection, flags, fd, offset) }
}

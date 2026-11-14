// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pread64`: the 64-bit `off_t` spelling of `pread` (ticket C66 5.F).
use core::ffi::{c_int, c_void};

/// Forward to `pread`.
///
/// # Safety
/// The caller's `pread` contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pread64(
    fd: c_int,
    buffer: *mut c_void,
    count: usize,
    offset: i64,
) -> isize {
    // SAFETY: forwarded from this function's contract.
    // SAFETY: the caller's buffer contract; a NULL with zero is the empty slice.
    let slice = unsafe { crate::ffi::cstr::slice_mut(buffer.cast(), count) };
    crate::ffi::ret::platform_ssize(crate::ffi::platform::global().pread(fd, slice, offset))
}

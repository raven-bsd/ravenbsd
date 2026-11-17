// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pwrite64`: the 64-bit `off_t` spelling of `pwrite` (ticket C66 5.F).
use core::ffi::{c_int, c_void};

/// Forward to `pwrite`.
///
/// # Safety
/// The caller's `pwrite` contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pwrite64(
    fd: c_int,
    buffer: *const c_void,
    count: usize,
    offset: i64,
) -> isize {
    // SAFETY: forwarded from this function's contract.
    // SAFETY: the caller's buffer contract; a NULL with zero is the empty slice.
    let slice = unsafe { crate::ffi::cstr::slice_n(buffer.cast(), count) };
    crate::ffi::ret::platform_ssize(crate::ffi::platform::global().pwrite(fd, slice, offset))
}

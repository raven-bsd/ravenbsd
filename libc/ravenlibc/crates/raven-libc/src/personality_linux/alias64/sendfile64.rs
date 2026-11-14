// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sendfile64`: the 64-bit `off_t` spelling of `sendfile` (ticket C66 5.F).
use core::ffi::c_int;

/// Forward to `sendfile`.
///
/// # Safety
/// The caller's `sendfile` contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sendfile64(
    out_fd: c_int,
    in_fd: c_int,
    offset: *mut i64,
    count: usize,
) -> isize {
    // SAFETY: forwarded from this function's contract.
    unsafe { crate::personality_linux::sendfile::sendfile(out_fd, in_fd, offset, count) }
}

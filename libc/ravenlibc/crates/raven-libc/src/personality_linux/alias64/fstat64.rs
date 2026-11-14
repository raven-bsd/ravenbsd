// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `fstat64`: the 64-bit `off_t` spelling of `fstat` (ticket C66 5.F).
use core::ffi::c_int;

use raven_libc_abi::structs::Stat;

/// Forward to `fstat`.
///
/// # Safety
/// The caller's `fstat` contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fstat64(fd: c_int, buf: *mut Stat) -> c_int {
    // SAFETY: forwarded from this function's contract.
    unsafe { crate::platform::fstat::fstat(fd, buf) }
}

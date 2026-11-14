// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `lstat64`: the 64-bit `off_t` spelling of `lstat` (ticket C66 5.F).
use core::ffi::{c_char, c_int};

use raven_libc_abi::structs::Stat;

/// Forward to `lstat`.
///
/// # Safety
/// The caller's `lstat` contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lstat64(path: *const c_char, buf: *mut Stat) -> c_int {
    // SAFETY: forwarded from this function's contract.
    unsafe { crate::platform::lstat::lstat(path, buf) }
}

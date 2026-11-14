// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `stat64`: the 64-bit `off_t` spelling of `stat` (ticket C66 5.F).
use core::ffi::{c_char, c_int};

use raven_libc_abi::structs::Stat;

/// Forward to `stat`; `struct stat` is already the 64-bit layout (DR-13).
///
/// # Safety
/// The caller's `stat` contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stat64(path: *const c_char, buf: *mut Stat) -> c_int {
    // SAFETY: forwarded from this function's contract.
    unsafe { crate::platform::stat::stat(path, buf) }
}

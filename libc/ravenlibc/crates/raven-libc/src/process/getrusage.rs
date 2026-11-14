// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getrusage(2)`.
use core::ffi::c_int;

use raven_libc_abi::structs::Rusage;

/// Zero the usage record until CPU accounting exists (O-8).
///
/// # Safety
/// `usage` is NULL or points to a writable `struct rusage`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getrusage(who: c_int, usage: *mut Rusage) -> c_int {
    if usage.is_null() {
        return crate::ffi::ret::int(Err(crate::process::exec_common::fault()));
    }
    if !matches!(who, -1..=1) {
        return crate::ffi::ret::int(Err(crate::ffi::map_proc::invalid()));
    }
    // SAFETY: the caller's writable `struct rusage`; accounting is zero until O-8.
    unsafe { usage.write(Rusage::ZERO) };
    0
}

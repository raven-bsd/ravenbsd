// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sigpending(3)`.
use core::ffi::c_int;

use raven_libc_signal::kernel::SysKernel;
use raven_libc_signal::{SigSet, maskops};

/// Store the thread-directed pending set.
///
/// # Safety
/// `set` points to a writable `sigset_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigpending(set: *mut SigSet) -> c_int {
    if set.is_null() {
        return -1;
    }
    match maskops::sigpending(&SysKernel, unsafe { &mut *set }) {
        Ok(()) => 0,
        Err(error) => {
            crate::ffi::errno::set(error);
            -1
        }
    }
}

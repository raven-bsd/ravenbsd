// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `clock_settime(2)`.
use core::ffi::{c_int, c_void};

use raven_libc_abi::structs::Timespec;

/// Set the clock `clk_id` to `tp`.
///
/// # Safety
/// `tp` is NULL or points to a readable `struct timespec`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clock_settime(clk_id: c_int, tp: *const c_void) -> c_int {
    if tp.is_null() {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    // SAFETY: `tp` is the caller's readable struct timespec.
    let value = unsafe { &*tp.cast::<Timespec>() };
    match raven_libc_time::clock::clock_settime(clk_id, value) {
        Ok(()) => 0,
        Err(error) => crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(error.0))),
    }
}

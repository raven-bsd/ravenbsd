// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `times(3)`.
use raven_libc_abi::errno::Errno;
use raven_libc_abi::limits::CLK_TCK;
use raven_libc_abi::time::CLOCK_MONOTONIC;

/// Fill the caller's `struct tms` and return the monotonic tick count.
///
/// # Safety
/// `tms` is NULL or points to a writable `struct tms`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn times(tms: *mut crate::ffi::map_proc::Tms) -> i64 {
    let now = match raven_libc_sys::sys::clock_get(CLOCK_MONOTONIC as u32) {
        Ok(value) => value,
        Err(error) => {
            crate::ffi::errno::set(Errno(error.0));
            return -1;
        }
    };
    let usec = now.tv_nsec / 1_000;
    match crate::ffi::map_proc::times(tms, now.tv_sec, usec, CLK_TCK as u32) {
        Ok(value) => value,
        Err(error) => crate::ffi::ret::long(Err(crate::ffi::map_proc::error(error))),
    }
}

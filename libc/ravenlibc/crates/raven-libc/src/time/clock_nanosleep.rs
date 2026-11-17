// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `clock_nanosleep` and the legacy `stime`.

use core::ffi::c_longlong;

use raven_libc_abi::structs::Timespec;

use super::ShellTime;

const TIMER_ABSTIME: i32 = 1;

/// `clock_nanosleep`: sleep against a relative duration or an absolute clock deadline.
///
/// # Safety
/// `request` is readable; `remaining` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clock_nanosleep(
    clock_id: i32,
    flags: i32,
    request: *const Timespec,
    remaining: *mut Timespec,
) -> i32 {
    if request.is_null() {
        return 22;
    }
    let absolute = flags & TIMER_ABSTIME != 0;
    // SAFETY: the C contract says `request` is readable.
    let req = unsafe { *request };
    match raven_libc_time::sleep::clock_nanosleep(&ShellTime, clock_id, absolute, &req) {
        Ok(()) => 0,
        Err(error) => {
            if !absolute && !remaining.is_null() {
                // SAFETY: the C contract says `remaining` is writable when non-NULL.
                unsafe { *remaining = error.rem };
            }
            i32::from(error.err.0)
        }
    }
}

/// `stime`: set the realtime clock from seconds since the epoch.
///
/// The platform clock is read-only, so this reports `EPERM` like the existing `clock_settime`.
///
/// # Safety
/// `seconds` is NULL or points to a readable `time_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stime(seconds: *const c_longlong) -> i32 {
    if seconds.is_null() {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(22)));
    }
    // SAFETY: the C contract says `seconds` is readable.
    let value = Timespec {
        tv_sec: unsafe { *seconds },
        tv_nsec: 0,
    };
    match raven_libc_time::clock::clock_settime(0, &value) {
        Ok(()) => 0,
        Err(error) => crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(error.0))),
    }
}

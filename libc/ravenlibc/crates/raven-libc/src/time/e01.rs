// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use core::ffi::{c_longlong, c_void};

use raven_libc_abi::structs::{Timespec, Timeval};

use super::ShellTime;

/// `time`.
///
/// # Safety
/// `out` is NULL or points to a writable `time_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn time(out: *mut c_longlong) -> c_longlong {
    match raven_libc_time::clock::time(&ShellTime) {
        Ok(value) => {
            if !out.is_null() {
                // SAFETY: the C contract says `out` is writable when non-NULL.
                unsafe { *out = value };
            }
            value
        }
        Err(error) => {
            crate::ffi::errno::set(error);
            -1
        }
    }
}

/// `clock_gettime`.
///
/// # Safety
/// `tp` is NULL or points to a writable `struct timespec`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clock_gettime(id: i32, tp: *mut Timespec) -> i32 {
    match raven_libc_time::clock::clock_gettime(&ShellTime, id) {
        Ok(value) => {
            if !tp.is_null() {
                // SAFETY: the C contract says `tp` is writable when non-NULL.
                unsafe { *tp = value };
            }
            0
        }
        Err(error) => crate::ffi::ret::int(Err(error)),
    }
}

/// `clock_getres`.
///
/// # Safety
/// `tp` is NULL or points to a writable `struct timespec`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clock_getres(id: i32, tp: *mut Timespec) -> i32 {
    match raven_libc_time::clock::clock_getres(id) {
        Ok(value) => {
            if !tp.is_null() {
                // SAFETY: the C contract says `tp` is writable when non-NULL.
                unsafe { *tp = value };
            }
            0
        }
        Err(error) => crate::ffi::ret::int(Err(error)),
    }
}

/// `gettimeofday`.
///
/// # Safety
/// `tv` is NULL or writable; `tz` is accepted but ignored.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gettimeofday(tv: *mut Timeval, _tz: *mut c_void) -> i32 {
    match raven_libc_time::clock::gettimeofday(&ShellTime) {
        Ok((sec, usec)) => {
            if !tv.is_null() {
                // SAFETY: the C contract says `tv` is writable when non-NULL.
                unsafe {
                    *tv = Timeval {
                        tv_sec: sec,
                        tv_usec: usec,
                    };
                }
            }
            0
        }
        Err(error) => crate::ffi::ret::int(Err(error)),
    }
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use core::ffi::c_longlong;
use core::ptr;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::structs::{Timespec, Tm as AbiTm};

use super::{convert, fill_abi_tm, platform, static_tm, utc};

/// `nanosleep`.
///
/// # Safety
/// `request` is readable and `remaining` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nanosleep(request: *const Timespec, remaining: *mut Timespec) -> i32 {
    if request.is_null() {
        return crate::ffi::ret::int(Err(Errno(22)));
    }
    // SAFETY: the C contract says `request` is readable.
    let req = unsafe { *request };
    if req.tv_sec < 0 || req.tv_nsec < 0 || req.tv_nsec >= 1_000_000_000 {
        return crate::ffi::ret::int(Err(Errno(22)));
    }
    let ns = (req.tv_sec as u64)
        .saturating_mul(1_000_000_000)
        .saturating_add(req.tv_nsec as u64);
    match raven_libc_platform::time::nanosleep(platform::global(), ns) {
        Ok(left) => {
            if !remaining.is_null() {
                // SAFETY: the C contract says `remaining` is writable when non-NULL.
                unsafe {
                    *remaining = Timespec {
                        tv_sec: (left / 1_000_000_000) as i64,
                        tv_nsec: (left % 1_000_000_000) as i64,
                    };
                }
            }
            0
        }
        Err(error) => crate::ffi::ret::int(Err(Errno(error.0))),
    }
}

/// `gmtime_r`.
///
/// # Safety
/// `timep` is readable and `out` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gmtime_r(timep: *const c_longlong, out: *mut AbiTm) -> *mut AbiTm {
    if timep.is_null() || out.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: the C contract says `timep` is readable.
    let seconds = unsafe { *timep };
    match convert::gmtime_r(seconds) {
        Ok(tm) => {
            // SAFETY: the C contract says `out` is writable.
            unsafe {
                fill_abi_tm(&mut *out, &tm);
            }
            out
        }
        Err(error) => crate::ffi::ret::null(error),
    }
}

/// `localtime_r`; the TZ environment hand-over is C61's, so this pass uses UTC.
///
/// # Safety
/// `timep` is readable and `out` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn localtime_r(timep: *const c_longlong, out: *mut AbiTm) -> *mut AbiTm {
    if timep.is_null() || out.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: the C contract says `timep` is readable.
    let seconds = unsafe { *timep };
    let state = utc();
    match convert::localtime_r(&state, seconds) {
        Ok(tm) => {
            // SAFETY: the C contract says `out` is writable.
            unsafe {
                fill_abi_tm(&mut *out, &tm);
            }
            out
        }
        Err(error) => crate::ffi::ret::null(error),
    }
}

/// `gmtime`.
///
/// # Safety
/// `timep` is readable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gmtime(timep: *const c_longlong) -> *mut AbiTm {
    if timep.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: the C contract says `timep` is readable.
    let seconds = unsafe { *timep };
    match convert::gmtime_r(seconds) {
        Ok(tm) => static_tm(&tm),
        Err(error) => crate::ffi::ret::null(error),
    }
}

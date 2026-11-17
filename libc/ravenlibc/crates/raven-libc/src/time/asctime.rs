// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `asctime`, `ctime`, `timespec_get` and `timespec_getres`.

use core::cell::UnsafeCell;
use core::ffi::{c_char, c_longlong};
use core::ptr;

use raven_libc_abi::structs::{Timespec, Tm as AbiTm};

use super::{ShellTime, convert, read_abi_tm, zone};

struct StaticText(UnsafeCell<[u8; 26]>);

// SAFETY: the shell is single-threaded; the C89 result object is overwritten by the next call.
unsafe impl Sync for StaticText {}

static ASCTIME_TEXT: StaticText = StaticText(UnsafeCell::new([0; 26]));

/// `asctime`: the fixed 26-byte text form in the C89 static buffer.
///
/// # Safety
/// `tm` is NULL or points to a readable `struct tm`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn asctime(tm: *const AbiTm) -> *mut c_char {
    if tm.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: the C contract says `tm` is readable.
    let value = read_abi_tm(unsafe { &*tm });
    let out = ASCTIME_TEXT.0.get();
    // SAFETY: single-threaded static cell; `asctime_r` writes 26 bytes or fails.
    match unsafe { raven_libc_time::asctime::asctime_r(&value, &mut *out) } {
        Ok(()) => out.cast::<c_char>(),
        Err(error) => {
            crate::ffi::errno::set(error);
            ptr::null_mut()
        }
    }
}

/// `ctime`: `asctime(localtime(t))`.
///
/// # Safety
/// `timep` is NULL or points to a readable `time_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ctime(timep: *const c_longlong) -> *mut c_char {
    if timep.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: the C contract says `timep` is readable.
    let seconds = unsafe { *timep };
    match zone::with_cache(|cache| convert::localtime_r(cache.state(), seconds)) {
        Ok(tm) => {
            let out = ASCTIME_TEXT.0.get();
            // SAFETY: single-threaded static cell; `asctime_r` writes 26 bytes or fails.
            match unsafe { raven_libc_time::asctime::asctime_r(&tm, &mut *out) } {
                Ok(()) => out.cast::<c_char>(),
                Err(error) => {
                    crate::ffi::errno::set(error);
                    ptr::null_mut()
                }
            }
        }
        Err(error) => {
            crate::ffi::errno::set(error);
            ptr::null_mut()
        }
    }
}

/// `timespec_get`: fill a timespec for `base` (only `TIME_UTC` is supported).
///
/// # Safety
/// `ts` is NULL or points to a writable `struct timespec`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn timespec_get(ts: *mut Timespec, base: i32) -> i32 {
    match raven_libc_time::clock::timespec_get(&ShellTime, base) {
        Ok(value) => {
            if !ts.is_null() {
                // SAFETY: the caller granted a writable timespec.
                unsafe { *ts = value };
            }
            base
        }
        Err(()) => 0,
    }
}

/// `timespec_getres`: the resolution of the `base` clock.
///
/// # Safety
/// `ts` is NULL or points to a writable `struct timespec`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn timespec_getres(ts: *mut Timespec, base: i32) -> i32 {
    if base != 1 {
        return 0;
    }
    match raven_libc_time::clock::clock_getres(0) {
        Ok(value) => {
            if !ts.is_null() {
                // SAFETY: the caller granted a writable timespec.
                unsafe { *ts = value };
            }
            base
        }
        Err(_) => 0,
    }
}

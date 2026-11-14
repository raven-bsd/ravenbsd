// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use core::ffi::{c_double, c_longlong};
use core::ptr;

use raven_libc_abi::structs::Tm as AbiTm;

use super::{convert, fill_abi_tm, read_abi_tm, static_tm, utc};

/// `localtime`.
///
/// # Safety
/// `timep` is readable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn localtime(timep: *const c_longlong) -> *mut AbiTm {
    if timep.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: the C contract says `timep` is readable.
    let seconds = unsafe { *timep };
    let state = utc();
    match convert::localtime_r(&state, seconds) {
        Ok(tm) => static_tm(&tm),
        Err(error) => crate::ffi::ret::null(error),
    }
}

/// `mktime`.
///
/// # Safety
/// `tm` is readable and writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mktime(tm: *mut AbiTm) -> c_longlong {
    if tm.is_null() {
        return -1;
    }
    // SAFETY: the C contract says `tm` is readable and writable.
    let source = unsafe { &mut *tm };
    let mut value = read_abi_tm(source);
    let state = utc();
    match raven_libc_time::tz::mktime::mktime(&state, &mut value) {
        Ok(seconds) => {
            fill_abi_tm(source, &value);
            seconds
        }
        Err(error) => {
            crate::ffi::errno::set(error);
            -1
        }
    }
}

/// `timegm`.
///
/// # Safety
/// `tm` is readable and writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn timegm(tm: *mut AbiTm) -> c_longlong {
    if tm.is_null() {
        return -1;
    }
    // SAFETY: the C contract says `tm` is readable and writable.
    let source = unsafe { &mut *tm };
    let mut value = read_abi_tm(source);
    match raven_libc_time::tz::mktime::timegm(&mut value) {
        Ok(seconds) => {
            fill_abi_tm(source, &value);
            seconds
        }
        Err(error) => {
            crate::ffi::errno::set(error);
            -1
        }
    }
}

/// `difftime`.
#[unsafe(no_mangle)]
pub extern "C" fn difftime(t1: c_longlong, t0: c_longlong) -> c_double {
    raven_libc_time::difftime::difftime(t1, t0)
}

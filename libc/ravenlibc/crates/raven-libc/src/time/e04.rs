// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use core::ffi::{c_char, c_long};
use core::ptr;

use raven_libc_abi::structs::Tm as AbiTm;
use raven_libc_time::locale::CLocale;

use super::{convert, fill_abi_tm, read_abi_tm, utc};

/// `strftime`.
///
/// # Safety
/// `out` is writable for `max` bytes, `format` is NUL terminated and `tm` is readable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strftime(
    out: *mut c_char,
    max: usize,
    format: *const c_char,
    tm: *const AbiTm,
) -> usize {
    if out.is_null() || format.is_null() || tm.is_null() {
        return 0;
    }
    // SAFETY: the C contract gives one NUL-terminated format string.
    let fmt = unsafe { crate::ffi::cstr::slice(format) };
    // SAFETY: the C contract gives a readable `tm`.
    let source = unsafe { &*tm };
    let value = read_abi_tm(source);
    let state = utc();
    let ctx = raven_libc_time::strftime::StrftimeCtx {
        tzname: [b"UTC", b"UTC"],
        localtime: Some(convert::localtime_r),
    };
    // SAFETY: the caller granted `max` writable bytes.
    let buffer = unsafe { core::slice::from_raw_parts_mut(out.cast::<u8>(), max) };
    raven_libc_time::strftime::strftime(&state, &CLocale, buffer, fmt, &value, &ctx)
}

/// `strptime`.
///
/// # Safety
/// `buf` and `format` are NUL terminated and `tm` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strptime(
    buf: *const c_char,
    format: *const c_char,
    tm: *mut AbiTm,
) -> *mut c_char {
    if buf.is_null() || format.is_null() || tm.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: both strings are NUL terminated by the C contract.
    let input = unsafe { crate::ffi::cstr::slice(buf) };
    let fmt = unsafe { crate::ffi::cstr::slice(format) };
    let source = unsafe { &mut *tm };
    let mut value = read_abi_tm(source);
    let state = utc();
    let ctx = raven_libc_time::strptime::StrptimeCtx {
        tzname: [b"UTC", b"UTC"],
        localtime: Some(convert::localtime_r),
    };
    match raven_libc_time::strptime::strptime(&state, &CLocale, input, fmt, &mut value, &ctx) {
        Some(rest) => {
            fill_abi_tm(source, &value);
            let offset = input.len().saturating_sub(rest.len());
            // SAFETY: `offset` is inside the caller's input buffer.
            unsafe { buf.add(offset).cast_mut() }
        }
        None => ptr::null_mut(),
    }
}

/// `clock`.
#[unsafe(no_mangle)]
pub extern "C" fn clock() -> c_long {
    -1
}

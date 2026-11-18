// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `tzset`, the `_l` time formatting forms and `wcsftime`.

use core::ffi::{c_char, c_void};

use raven_libc_abi::structs::Tm as AbiTm;
use raven_libc_time::locale::CLocale;

use super::{convert, read_abi_tm, zone};

/// `tzset`: reload the process `TZ` cache and publish `tzname`/`timezone`/`daylight`.
#[unsafe(no_mangle)]
pub extern "C" fn tzset() {
    zone::reload();
}

/// `strftime_l`: `strftime` with a locale object (the object is accepted and ignored).
///
/// # Safety
/// The `strftime` buffer/string contract applies.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strftime_l(
    out: *mut c_char,
    max: usize,
    format: *const c_char,
    tm: *const AbiTm,
    _locale: *mut c_void,
) -> usize {
    // SAFETY: forwarded C contract.
    unsafe { super::strftime::strftime(out, max, format, tm) }
}

/// `strptime_l`: `strptime` with a locale object (the object is accepted and ignored).
///
/// # Safety
/// The `strptime` buffer/string contract applies.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strptime_l(
    buf: *const c_char,
    format: *const c_char,
    tm: *mut AbiTm,
    _locale: *mut c_void,
) -> *mut c_char {
    // SAFETY: forwarded C contract.
    unsafe { super::strftime::strptime(buf, format, tm) }
}

/// `wcsftime`: format wide text through the UTF-8 `strftime` engine.
///
/// # Safety
/// `out` has `max` writable `wchar_t` units, `format` is NUL-terminated and `tm` is readable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcsftime(
    out: *mut u32,
    max: usize,
    format: *const u32,
    tm: *const AbiTm,
) -> usize {
    if out.is_null() || format.is_null() || tm.is_null() || max == 0 {
        return 0;
    }
    let mut length = 0usize;
    // SAFETY: the C contract gives a NUL-terminated wide format string.
    while unsafe { *format.add(length) } != 0 {
        length += 1;
    }
    // SAFETY: the loop above bounded the format by its NUL terminator.
    let fmt = unsafe { core::slice::from_raw_parts(format, length) };
    // SAFETY: the caller granted `max` writable wide units.
    let buffer = unsafe { core::slice::from_raw_parts_mut(out, max) };
    // SAFETY: the caller granted a readable `tm`.
    let source = unsafe { &*tm };
    let value = read_abi_tm(source);
    let gmt = source.tm_zone == zone::gmt_pointer().cast::<core::ffi::c_void>();
    zone::with_cache(|cache| {
        let state = cache.state();
        let names = cache.tzname();
        let ctx = if gmt {
            raven_libc_time::strftime::StrftimeCtx {
                tzname: [b"GMT", b"GMT"],
                localtime: Some(convert::localtime_r),
            }
        } else {
            raven_libc_time::strftime::StrftimeCtx {
                tzname: names,
                localtime: Some(convert::localtime_r),
            }
        };
        raven_libc_time::wcsftime::wcsftime(state, &CLocale, buffer, fmt, &value, &ctx)
    })
}

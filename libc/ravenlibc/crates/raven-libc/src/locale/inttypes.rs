// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `umaxabs`, `wcstoimax` and `wcstoumax` from `<inttypes.h>`.

use core::ffi::c_int;

use raven_libc_abi::errno::Errno;
use raven_libc_stdlib::IntStatus;
use raven_libc_stdlib::conv::scan;

/// `umaxabs`: the absolute value of an `intmax_t` as a `uintmax_t`.
#[unsafe(no_mangle)]
pub extern "C" fn umaxabs(value: i64) -> u64 {
    value.unsigned_abs()
}

/// The length of a NUL-terminated wide string, capped at one million code units.
///
/// # Safety
/// `pointer` is a NUL-terminated `wchar_t` array.
unsafe fn wide_length(pointer: *const u32) -> usize {
    let mut length = 0usize;
    while length < 1_048_576 {
        // SAFETY: the caller's NUL-terminated array.
        if unsafe { *pointer.add(length) } == 0 {
            break;
        }
        length = length.saturating_add(1);
    }
    length
}

/// `wcstoimax`.
///
/// # Safety
/// `nptr` is NULL or a NUL-terminated `wchar_t` string; `endptr` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcstoimax(
    nptr: *const c_int,
    endptr: *mut *mut c_int,
    base: c_int,
) -> i64 {
    if nptr.is_null() {
        if !endptr.is_null() {
            // SAFETY: the caller's out pointer.
            unsafe { *endptr = core::ptr::null_mut() };
        }
        return 0;
    }
    let pointer = nptr.cast::<u32>();
    // SAFETY: the caller's wide string.
    let length = unsafe { wide_length(pointer) };
    // SAFETY: the caller's readable wide array.
    let input = unsafe { core::slice::from_raw_parts(pointer, length) };
    let result = scan(input, base, true);
    if !endptr.is_null() {
        // SAFETY: `consumed <= length`, so the offset stays in the caller's array.
        unsafe { *endptr = pointer.add(result.consumed).cast_mut().cast::<c_int>() };
    }
    match result.status {
        IntStatus::InvalidBase => {
            crate::ffi::errno::set(Errno(22));
            0
        }
        IntStatus::Range => {
            crate::ffi::errno::set(Errno(34));
            if result.negative { i64::MIN } else { i64::MAX }
        }
        _ => {
            if result.negative {
                (result.value as i64).wrapping_neg()
            } else {
                result.value as i64
            }
        }
    }
}

/// `wcstoumax`.
///
/// # Safety
/// `nptr` is NULL or a NUL-terminated `wchar_t` string; `endptr` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcstoumax(
    nptr: *const c_int,
    endptr: *mut *mut c_int,
    base: c_int,
) -> u64 {
    if nptr.is_null() {
        if !endptr.is_null() {
            // SAFETY: the caller's out pointer.
            unsafe { *endptr = core::ptr::null_mut() };
        }
        return 0;
    }
    let pointer = nptr.cast::<u32>();
    // SAFETY: the caller's wide string.
    let length = unsafe { wide_length(pointer) };
    // SAFETY: the caller's readable wide array.
    let input = unsafe { core::slice::from_raw_parts(pointer, length) };
    let result = scan(input, base, false);
    if !endptr.is_null() {
        // SAFETY: `consumed <= length`, so the offset stays in the caller's array.
        unsafe { *endptr = pointer.add(result.consumed).cast_mut().cast::<c_int>() };
    }
    match result.status {
        IntStatus::InvalidBase => {
            crate::ffi::errno::set(Errno(22));
            0
        }
        IntStatus::Range => {
            crate::ffi::errno::set(Errno(34));
            u64::MAX
        }
        _ => {
            if result.negative {
                result.value.wrapping_neg()
            } else {
                result.value
            }
        }
    }
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `strtoul(3)`, `strtoull(3)` and `strtoumax(3)`.
use core::ffi::{c_char, c_int, c_ulong};

/// Parse an unsigned integer; `endptr` is written only when non-NULL.
///
/// # Safety
/// `s` is a NUL-terminated string; `endptr` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strtoul(
    s: *const c_char,
    endptr: *mut *mut c_char,
    base: c_int,
) -> c_ulong {
    // SAFETY: the caller's string contract.
    let bytes = unsafe { crate::ffi::cstr::slice(s) };
    let (value, used, status) = raven_libc_stdlib::conv::strtoul(bytes, base);
    if !endptr.is_null() {
        // SAFETY: the caller grants a writable pointer slot; `used <= bytes.len()`.
        unsafe { *endptr = s.add(used).cast_mut() };
    }
    crate::ffi::map_int::apply(status);
    value
}

/// The `unsigned long long` form.
///
/// # Safety
/// As `strtoul`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strtoull(
    s: *const c_char,
    endptr: *mut *mut c_char,
    base: c_int,
) -> c_ulong {
    // SAFETY: as `strtoul`.
    unsafe { strtoul(s, endptr, base) }
}

/// The `uintmax_t` form.
///
/// # Safety
/// As `strtoul`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strtoumax(
    s: *const c_char,
    endptr: *mut *mut c_char,
    base: c_int,
) -> c_ulong {
    // SAFETY: as `strtoul`.
    unsafe { strtoul(s, endptr, base) }
}

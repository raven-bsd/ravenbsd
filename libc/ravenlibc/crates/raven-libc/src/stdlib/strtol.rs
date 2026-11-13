// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `strtol(3)`, `strtoll(3)` and `strtoimax(3)`.
use core::ffi::{c_char, c_int, c_long};

/// Parse a signed integer; `endptr` is written only when non-NULL.
///
/// # Safety
/// `s` is a NUL-terminated string; `endptr` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strtol(s: *const c_char, endptr: *mut *mut c_char, base: c_int) -> c_long {
    // SAFETY: the caller's string contract.
    let bytes = unsafe { crate::ffi::cstr::slice(s) };
    let (value, used, status) = raven_libc_stdlib::conv::strtol(bytes, base);
    if !endptr.is_null() {
        // SAFETY: the caller grants a writable pointer slot; `used <= bytes.len()`.
        unsafe { *endptr = s.add(used).cast_mut() };
    }
    crate::ffi::map_int::apply(status);
    value
}

/// The `long long` form (64-bit on every target).
///
/// # Safety
/// As `strtol`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strtoll(
    s: *const c_char,
    endptr: *mut *mut c_char,
    base: c_int,
) -> c_long {
    // SAFETY: as `strtol`.
    unsafe { strtol(s, endptr, base) }
}

/// The `intmax_t` form.
///
/// # Safety
/// As `strtol`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strtoimax(
    s: *const c_char,
    endptr: *mut *mut c_char,
    base: c_int,
) -> c_long {
    // SAFETY: as `strtol`.
    unsafe { strtol(s, endptr, base) }
}

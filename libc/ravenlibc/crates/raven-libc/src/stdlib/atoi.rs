// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `atoi(3)`, `atol(3)` and `atoll(3)`.
use core::ffi::{c_char, c_int, c_long};

/// Parse a decimal `int`; no error reporting.
///
/// # Safety
/// `s` is a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn atoi(s: *const c_char) -> c_int {
    // SAFETY: the caller's string contract.
    raven_libc_stdlib::conv::atoi(unsafe { crate::ffi::cstr::slice(s) })
}

/// Parse a decimal `long`.
///
/// # Safety
/// `s` is a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn atol(s: *const c_char) -> c_long {
    // SAFETY: the caller's string contract.
    raven_libc_stdlib::conv::atol(unsafe { crate::ffi::cstr::slice(s) })
}

/// Parse a decimal `long long`.
///
/// # Safety
/// `s` is a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn atoll(s: *const c_char) -> c_long {
    // SAFETY: the caller's string contract.
    raven_libc_stdlib::conv::atoll(unsafe { crate::ffi::cstr::slice(s) })
}

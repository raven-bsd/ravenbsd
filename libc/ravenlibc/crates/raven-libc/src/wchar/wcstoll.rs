// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `wcstoll`, `wcstoull`, `wcstoimax` and `wcstoumax`.

use super::wcstod::{wcstol, wcstoul};

/// `wcstoll`.
///
/// # Safety
/// As `wcstol`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcstoll(
    input: *const u32,
    endptr: *mut *mut u32,
    base: core::ffi::c_int,
) -> i64 {
    // SAFETY: forwarded C contract.
    unsafe { wcstol(input, endptr, base) }
}

/// `wcstoull`.
///
/// # Safety
/// As `wcstoul`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcstoull(
    input: *const u32,
    endptr: *mut *mut u32,
    base: core::ffi::c_int,
) -> u64 {
    // SAFETY: forwarded C contract.
    unsafe { wcstoul(input, endptr, base) }
}

/// `wcstoimax`.
///
/// # Safety
/// As `wcstol`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcstoimax(
    input: *const u32,
    endptr: *mut *mut u32,
    base: core::ffi::c_int,
) -> i64 {
    // SAFETY: forwarded C contract.
    unsafe { wcstol(input, endptr, base) }
}

/// `wcstoumax`.
///
/// # Safety
/// As `wcstoul`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcstoumax(
    input: *const u32,
    endptr: *mut *mut u32,
    base: core::ffi::c_int,
) -> u64 {
    // SAFETY: forwarded C contract.
    unsafe { wcstoul(input, endptr, base) }
}

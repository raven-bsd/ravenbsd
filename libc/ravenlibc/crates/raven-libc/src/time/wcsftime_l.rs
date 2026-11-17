// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `wcsftime_l`.

use core::ffi::c_void;

use raven_libc_abi::structs::Tm as AbiTm;

/// `wcsftime_l`: `wcsftime` with a locale object (the object is accepted and ignored).
///
/// # Safety
/// The `wcsftime` wide buffer/string contract applies.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcsftime_l(
    out: *mut u32,
    max: usize,
    format: *const u32,
    tm: *const AbiTm,
    _locale: *mut c_void,
) -> usize {
    // SAFETY: forwarded C contract.
    unsafe { super::tzset::wcsftime(out, max, format, tm) }
}

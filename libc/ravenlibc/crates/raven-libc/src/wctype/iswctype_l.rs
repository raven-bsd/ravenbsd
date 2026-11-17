// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `iswctype_l`, `towlower_l`, `towupper_l` and `towctrans_l`.

use core::ffi::{c_int, c_void};

/// Locale-aware named classification.
#[unsafe(no_mangle)]
pub extern "C" fn iswctype_l(wc: u32, kind: u64, _locale: *mut c_void) -> c_int {
    c_int::from(raven_libc_ctype::iswctype(wc, kind))
}

/// Locale-aware lowercase mapping.
#[unsafe(no_mangle)]
pub extern "C" fn towlower_l(wc: u32, _locale: *mut c_void) -> u32 {
    raven_libc_ctype::towlower(wc)
}

/// Locale-aware uppercase mapping.
#[unsafe(no_mangle)]
pub extern "C" fn towupper_l(wc: u32, _locale: *mut c_void) -> u32 {
    raven_libc_ctype::towupper(wc)
}

/// Locale-aware named mapping.
#[unsafe(no_mangle)]
pub extern "C" fn towctrans_l(wc: u32, trans: u64, _locale: *mut c_void) -> u32 {
    raven_libc_ctype::towctrans(wc, trans)
}

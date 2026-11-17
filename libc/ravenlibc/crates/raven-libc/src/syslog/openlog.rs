// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `openlog` from `<syslog.h>`.

use core::ffi::{c_char, c_int};
use core::sync::atomic::Ordering;

use super::{FACILITY, IDENT, OPTION};

/// `openlog`.
///
/// # Safety
/// `ident` is NULL or NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn openlog(
    ident_ptr: *const c_char,
    option_value: c_int,
    facility_value: c_int,
) {
    if !ident_ptr.is_null() {
        // SAFETY: the caller's C string contract.
        let bytes = unsafe { crate::ffi::cstr::slice(ident_ptr) };
        // SAFETY: single-threaded access.
        let cell = unsafe { &mut *IDENT.0.get() };
        cell.clear();
        cell.extend_from_slice(bytes);
    }
    OPTION.store(option_value, Ordering::Relaxed);
    FACILITY.store(facility_value, Ordering::Relaxed);
}

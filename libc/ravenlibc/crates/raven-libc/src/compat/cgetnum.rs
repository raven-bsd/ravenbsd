// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the cgetnum export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `cgetnum` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

use raven_libc_compat::getcap;

/// `cgetnum`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cgetnum(
    buffer: *mut c_char,
    cap: *const c_char,
    number: *mut core::ffi::c_long,
) -> c_int {
    if buffer.is_null() || cap.is_null() || number.is_null() {
        return -1;
    }
    // SAFETY: both C strings are NUL-terminated.
    let record = unsafe { super::c_bytes(buffer, super::MAX_CSTR) };
    let cap = unsafe { super::c_bytes(cap, super::MAX_NAME) };
    match getcap::num(record, cap) {
        Ok(value) => {
            // SAFETY: `number` is writable.
            unsafe { *number = value as core::ffi::c_long };
            0
        }
        Err(_) => -1,
    }
}

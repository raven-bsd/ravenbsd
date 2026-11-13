// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the scan_scaled export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `scan_scaled` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

use raven_libc_compat::humanize;

/// `scan_scaled`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scan_scaled(text: *const c_char, number: *mut i64) -> c_int {
    if text.is_null() || number.is_null() {
        // SAFETY: the errno cell is this thread's.
        unsafe { super::set_errno(super::EINVAL) };
        return -1;
    }
    // SAFETY: `text` is NUL-terminated.
    let text = unsafe { super::c_bytes(text, super::MAX_CSTR) };
    match humanize::scan_scaled(text) {
        Ok(value) => {
            // SAFETY: `number` is writable.
            unsafe { *number = value };
            0
        }
        Err(error) => {
            // SAFETY: the errno cell is this thread's.
            unsafe { super::set_errno(c_int::from(error.0)) };
            -1
        }
    }
}

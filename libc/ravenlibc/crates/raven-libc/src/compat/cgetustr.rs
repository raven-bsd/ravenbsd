// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the cgetustr export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `cgetustr` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

use raven_libc_compat::getcap;

/// `cgetustr`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cgetustr(
    buffer: *mut c_char,
    cap: *const c_char,
    string: *mut *mut c_char,
) -> c_int {
    if buffer.is_null() || cap.is_null() || string.is_null() {
        return -1;
    }
    // SAFETY: both C strings are NUL-terminated.
    let record = unsafe { super::c_bytes(buffer, super::MAX_CSTR) };
    let cap = unsafe { super::c_bytes(cap, super::MAX_NAME) };
    match getcap::ustr(record, cap) {
        Ok(value) => {
            let pointer = unsafe { super::alloc_cstr(&value) };
            if pointer.is_null() {
                return -2;
            }
            // SAFETY: `string` is writable.
            unsafe { *string = pointer };
            0
        }
        Err(_) => -1,
    }
}

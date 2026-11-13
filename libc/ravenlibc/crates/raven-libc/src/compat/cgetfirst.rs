// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the cgetfirst export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `cgetfirst` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

use super::getcap_state::{RealFiles, db, error_code, files};

/// `cgetfirst`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cgetfirst(buffer: *mut *mut c_char, db_array: *mut *mut c_char) -> c_int {
    if buffer.is_null() {
        return -1;
    }
    // SAFETY: the caller passes a NULL-terminated file array.
    let files = unsafe { files(db_array) };
    match unsafe { db() }.first(&mut RealFiles, &files) {
        Ok(record) => {
            let pointer = unsafe { super::alloc_cstr(&record) };
            if pointer.is_null() {
                return -1;
            }
            // SAFETY: `buffer` is writable.
            unsafe { *buffer = pointer };
            1
        }
        Err(error) => error_code(error),
    }
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the cgetent export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `cgetent` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

use super::getcap_state::{RealFiles, db, error_code, files};

/// `cgetent`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cgetent(
    buffer: *mut *mut c_char,
    db_array: *mut *mut c_char,
    name: *const c_char,
) -> c_int {
    if buffer.is_null() || name.is_null() {
        return -1;
    }
    // SAFETY: the caller passes a NULL-terminated file array and a name.
    let files = unsafe { files(db_array) };
    let name = unsafe { super::c_bytes(name, super::MAX_NAME) };
    let result = unsafe { db() }.ent(&mut RealFiles, &files, name);
    match result {
        Ok(record) => {
            let pointer = unsafe { super::alloc_cstr(&record) };
            if pointer.is_null() {
                return -2;
            }
            // SAFETY: `buffer` is writable.
            unsafe { *buffer = pointer };
            0
        }
        Err(error) => error_code(error),
    }
}

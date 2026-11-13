// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the crypt export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `crypt` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

static mut CRYPT_BUFFER: [u8; 256] = [0; 256];

/// `crypt`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn crypt(key: *const c_char, salt: *const c_char) -> *mut c_char {
    if key.is_null() || salt.is_null() {
        // SAFETY: the errno cell is this thread's.
        unsafe { super::set_errno(super::EINVAL) };
        return core::ptr::null_mut();
    }
    // SAFETY: both inputs are NUL-terminated C strings.
    let key = unsafe { super::c_bytes(key, super::MAX_CSTR) };
    let salt = unsafe { super::c_bytes(salt, 256) };
    // SAFETY: the static buffer is the donor's own per-process storage.
    let buffer = unsafe { &mut *(&raw mut CRYPT_BUFFER) };
    match raven_libc_compat::crypt::crypt_r(key, salt, buffer) {
        Ok(_) => buffer.as_mut_ptr().cast::<c_char>(),
        Err(error) => {
            // SAFETY: the errno cell is this thread's.
            unsafe { super::set_errno(c_int::from(error.0)) };
            core::ptr::null_mut()
        }
    }
}

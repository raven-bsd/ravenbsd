// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the crypt_r export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `crypt_r` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

use raven_libc_compat::crypt::crypt_r as logic;

/// The musl `struct crypt_data` shape.
#[repr(C)]
pub struct CryptData {
    pub initialized: c_int,
    pub __buf: [u8; 256],
}

/// `crypt_r`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn crypt_r(
    key: *const c_char,
    salt: *const c_char,
    data: *mut CryptData,
) -> *mut c_char {
    if key.is_null() || salt.is_null() || data.is_null() {
        // SAFETY: the errno cell is this thread's.
        unsafe { super::set_errno(super::EINVAL) };
        return core::ptr::null_mut();
    }
    // SAFETY: both inputs are NUL-terminated C strings.
    let key = unsafe { super::c_bytes(key, super::MAX_CSTR) };
    let salt = unsafe { super::c_bytes(salt, 256) };
    // SAFETY: `data` is the caller's struct crypt_data.
    let buffer = unsafe { &mut (*data).__buf };
    match logic(key, salt, buffer) {
        Ok(_) => {
            // SAFETY: `data` is live.
            unsafe {
                (*data).initialized = 1;
                (*data).__buf.as_mut_ptr().cast::<c_char>()
            }
        }
        Err(error) => {
            // SAFETY: the errno cell is this thread's.
            unsafe { super::set_errno(c_int::from(error.0)) };
            core::ptr::null_mut()
        }
    }
}

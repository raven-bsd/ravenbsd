// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `inet_aton(3)`.
use core::ffi::{c_char, c_int};

use crate::ffi::sockaddr::InAddr;

/// Parse a dotted quad into the caller's `struct in_addr`.
///
/// # Safety
/// `text` is NUL-terminated; `address` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inet_aton(text: *const c_char, address: *mut InAddr) -> c_int {
    if text.is_null() || address.is_null() {
        return 0;
    }
    // SAFETY: the caller's NUL-terminated string.
    match raven_libc_resolv::inet_aton(unsafe { crate::ffi::cstr::slice(text) }) {
        Some(bytes) => {
            // SAFETY: the caller's writable address.
            unsafe {
                address.write(InAddr {
                    s_addr: u32::from_ne_bytes(bytes),
                })
            };
            1
        }
        None => 0,
    }
}

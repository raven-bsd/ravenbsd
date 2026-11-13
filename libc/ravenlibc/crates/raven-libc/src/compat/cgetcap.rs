// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the cgetcap export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `cgetcap` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

use raven_libc_compat::getcap;

/// `cgetcap`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cgetcap(
    buffer: *mut c_char,
    cap: *const c_char,
    kind: c_int,
) -> *mut c_char {
    if buffer.is_null() || cap.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: `buffer` is NUL-terminated and `cap` is a capability name.
    let record = unsafe { super::c_bytes(buffer, super::MAX_CSTR) };
    let cap = unsafe { super::c_bytes(cap, super::MAX_NAME) };
    match getcap::cap(record, cap, kind as u8) {
        // SAFETY: the offset is inside the caller's buffer.
        Some(offset) => unsafe { buffer.add(offset) },
        None => core::ptr::null_mut(),
    }
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the setmntent export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `setmntent` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

/// `setmntent`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn setmntent(name: *const c_char, mode: *const c_char) -> *mut super::File {
    if name.is_null() || mode.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: both arguments are NUL-terminated.
    unsafe { super::fopen(name, mode) }
}

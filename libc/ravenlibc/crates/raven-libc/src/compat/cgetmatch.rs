// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the cgetmatch export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `cgetmatch` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

use raven_libc_compat::getcap;

/// `cgetmatch`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cgetmatch(buffer: *const c_char, name: *const c_char) -> c_int {
    if buffer.is_null() || name.is_null() {
        return -1;
    }
    // SAFETY: both arguments are NUL-terminated.
    let record = unsafe { super::c_bytes(buffer, super::MAX_CSTR) };
    let name = unsafe { super::c_bytes(name, super::MAX_NAME) };
    if getcap::matches(record, name) { 0 } else { -1 }
}

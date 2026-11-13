// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the cgetset export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `cgetset` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

use super::getcap_state::{RealFiles, db, error_code, files};

/// `cgetset`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cgetset(entry: *const c_char) -> c_int {
    if entry.is_null() {
        // SAFETY: the static CapDb is the donor's global state.
        return match unsafe { db() }.set(None) {
            Ok(()) => 0,
            Err(_) => -1,
        };
    }
    // SAFETY: `entry` is NUL-terminated.
    let entry = unsafe { super::c_bytes(entry, super::MAX_CSTR) };
    // SAFETY: the static CapDb is the donor's global state.
    match unsafe { db() }.set(Some(entry)) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

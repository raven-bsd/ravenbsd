// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the getttynam export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `getttynam` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

use super::ttyent_state::{CTtyEnt, TTYDB, fill, load};

/// `getttynam`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getttynam(tty: *const c_char) -> *mut CTtyEnt {
    if tty.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: `tty` is NUL-terminated.
    let name = unsafe { super::c_bytes(tty, super::MAX_NAME) };
    // SAFETY: the static is private; the C API is not re-entrant.
    let slot = unsafe { &raw mut TTYDB };
    if unsafe { (*slot).is_none() } && !unsafe { load() } {
        return core::ptr::null_mut();
    }
    match unsafe { (*slot).as_ref() } {
        Some(database) => match database.by_name(name) {
            Some(entry) => unsafe { fill(&entry) },
            None => core::ptr::null_mut(),
        },
        None => core::ptr::null_mut(),
    }
}

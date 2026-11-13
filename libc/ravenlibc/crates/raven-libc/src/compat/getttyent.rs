// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the getttyent export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `getttyent` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

use super::ttyent_state::{CTtyEnt, TTYDB, fill, load};

/// `getttyent`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getttyent() -> *mut CTtyEnt {
    // SAFETY: the static is private; the C API is not re-entrant.
    let slot = unsafe { &raw mut TTYDB };
    if unsafe { (*slot).is_none() } && !unsafe { load() } {
        return core::ptr::null_mut();
    }
    match unsafe { (*slot).as_mut() } {
        Some(database) => match database.next() {
            Some(entry) => unsafe { fill(&entry) },
            None => core::ptr::null_mut(),
        },
        None => core::ptr::null_mut(),
    }
}

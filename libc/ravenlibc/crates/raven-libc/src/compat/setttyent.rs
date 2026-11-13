// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the setttyent export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `setttyent` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

use super::ttyent_state::{CTtyEnt, TTYDB, fill, load};

/// `setttyent`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn setttyent() -> c_int {
    // SAFETY: the C API is not re-entrant.
    if unsafe { load() } { 1 } else { 0 }
}

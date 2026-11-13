// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the endttyent export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `endttyent` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

use super::ttyent_state::{CTtyEnt, TTYDB, fill, load};

/// `endttyent`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn endttyent() -> c_int {
    // SAFETY: the static is private; clearing it drops the parsed database.
    unsafe { *&raw mut TTYDB = None };
    1
}

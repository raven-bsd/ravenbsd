// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the cgetclose export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `cgetclose` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

use super::getcap_state::{RealFiles, db, error_code, files};

/// `cgetclose`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cgetclose() -> c_int {
    // SAFETY: the static CapDb is the donor's global state.
    unsafe { db() }.close();
    0
}

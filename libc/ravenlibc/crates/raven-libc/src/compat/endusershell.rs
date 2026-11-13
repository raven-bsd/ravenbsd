// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the endusershell export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `endusershell` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

use super::usershell_state::{SHELL_BUF, SHELLS, init};

/// `endusershell`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn endusershell() {
    // SAFETY: the static is private; clearing it drops the list.
    unsafe { *&raw mut SHELLS = None };
}

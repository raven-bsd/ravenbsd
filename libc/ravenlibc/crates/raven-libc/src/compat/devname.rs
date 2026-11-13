// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the devname export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `devname` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

/// `devname`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn devname(_dev: u64, _mode: u32) -> *mut c_char {
    core::ptr::null_mut()
}

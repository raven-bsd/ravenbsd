// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the getutxid export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `getutxid` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

/// `getutxid`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getutxid(_id: *const core::ffi::c_void) -> *mut core::ffi::c_void {
    // SAFETY: the errno cell is this thread's.
    unsafe { super::set_errno(super::ENOENT) };
    core::ptr::null_mut()
}

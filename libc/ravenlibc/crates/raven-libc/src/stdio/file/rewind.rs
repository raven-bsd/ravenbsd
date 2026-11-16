// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `rewind` C-ABI export.

use core::ffi::c_void;

/// `rewind`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rewind(stream: *mut c_void) {
    // SAFETY: forwarded C contract.
    unsafe { super::rewind_impl(stream) }
}

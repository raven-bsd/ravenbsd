// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `timer_getoverrun`.

use core::ffi::{c_int, c_void};

/// `timer_getoverrun`.
///
/// # Safety
/// `timer` is a live timer_t.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn timer_getoverrun(timer: *mut c_void) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::timer_create::timer_getoverrun_impl(timer) }
}

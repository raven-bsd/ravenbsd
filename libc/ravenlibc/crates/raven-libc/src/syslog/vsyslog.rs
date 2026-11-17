// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `vsyslog`, the `syslog` trampoline target.

use core::ffi::{c_char, c_int};

use crate::stdio::va::List;

/// `vsyslog`.
///
/// # Safety
/// `format` is NUL terminated; `list` was built by the `syslog` trampoline.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vsyslog(priority: c_int, format: *const c_char, list: *mut List) {
    // SAFETY: forwarded caller/trampoline contract.
    unsafe { super::emit(priority, format, list) }
}

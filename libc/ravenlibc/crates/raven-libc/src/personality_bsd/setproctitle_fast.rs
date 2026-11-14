// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `setproctitle_fast(3)`.
use core::ffi::c_char;

use crate::stdio::va::List;

crate::raven_variadic!(
    prefix: "",
    export: "setproctitle_fast",
    target: crate::personality_bsd::setproctitle_fast::__raven_va_setproctitle_fast,
    named: 1
);

/// The trampoline target; skips the `procd` call when the title is cached.
///
/// # Safety
/// The trampoline built `list`; `format` is NULL or NUL terminated.
pub unsafe extern "C" fn __raven_va_setproctitle_fast(format: *const c_char, list: *mut List) {
    // SAFETY: forwarded from this function's contract.
    unsafe { crate::personality_bsd::setproctitle::run(true, format, list) };
}

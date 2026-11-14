// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `error_print_progname`, the one function-pointer data symbol of `error.h`.

/// `void (*error_print_progname)(void)`.
#[unsafe(no_mangle)]
pub static mut error_print_progname: Option<unsafe extern "C" fn()> = None;

/// The current hook.
pub(crate) fn hook() -> Option<unsafe extern "C" fn()> {
    // SAFETY: `addr_of!` forms the address without a reference to a `static mut`.
    unsafe { core::ptr::read(core::ptr::addr_of!(error_print_progname)) }
}

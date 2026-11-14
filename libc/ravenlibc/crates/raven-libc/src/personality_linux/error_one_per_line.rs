// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `error_one_per_line`, the suppression switch of `error.h`.
use core::ffi::c_int;

/// `int error_one_per_line`.
#[unsafe(no_mangle)]
pub static mut error_one_per_line: c_int = 0;

/// The current value.
pub(crate) fn get() -> c_int {
    // SAFETY: scalar read through `addr_of!`.
    unsafe { core::ptr::read(core::ptr::addr_of!(error_one_per_line)) }
}

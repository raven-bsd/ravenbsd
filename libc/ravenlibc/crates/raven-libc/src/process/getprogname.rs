// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getprogname(3)`.
use core::ffi::c_char;

/// The program basename set by start or `setprogname` (never NULL).
#[unsafe(no_mangle)]
pub extern "C" fn getprogname() -> *const c_char {
    // SAFETY: `__progname` is the shell's own data symbol; a concurrent writer cannot exist
    // until C57.
    let pointer = unsafe { core::ptr::read(&raw const crate::process::__progname::__progname) };
    if pointer.is_null() {
        c"".as_ptr()
    } else {
        pointer
    }
}

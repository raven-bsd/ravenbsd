// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `setprogname(3)`.
use core::ffi::c_char;

/// Set the program name and its basename.
///
/// # Safety
/// `name` is NULL or a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn setprogname(name: *const c_char) {
    let bytes = if name.is_null() {
        b"".as_slice()
    } else {
        // SAFETY: the caller's NUL-terminated string.
        unsafe { crate::ffi::cstr::slice(name) }
    };
    let short = raven_libc_proc::setprogname::setprogname(bytes);
    // SAFETY: the shell owns the two data symbols; `short` points into `bytes`.
    unsafe {
        crate::process::__progname_full::__progname_full = name;
        crate::process::__progname::__progname = short.as_ptr().cast::<c_char>();
    }
}

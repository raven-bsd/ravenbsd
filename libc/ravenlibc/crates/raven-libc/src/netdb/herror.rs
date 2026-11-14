// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `herror(3)`.
use core::ffi::c_char;

/// Write `message: hstrerror(h_errno)` to descriptor 2.
///
/// # Safety
/// `message` is NULL or a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn herror(message: *const c_char) {
    // SAFETY: the caller's optional NUL-terminated message.
    let text = unsafe { crate::ffi::cstr::slice(message) };
    // SAFETY: the shell's own `h_errno` data symbol.
    let code = unsafe { core::ptr::read(&raw const crate::netdb::h_errno::h_errno) };
    let line = crate::ffi::map_net::herror_line(text, code);
    let _ = crate::netdb::write::write_all(2, &line);
}

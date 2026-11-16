// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `readlink(2)`.
use core::ffi::{c_char, c_int};

/// Read the target of `path` into `buf`.
///
/// # Safety
/// `path` is NULL or a NUL-terminated string; `buf` is NULL or `bufsize` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn readlink(path: *const c_char, buf: *mut c_char, bufsize: usize) -> isize {
    // SAFETY: the caller's C string contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    // SAFETY: the caller's buffer contract.
    let buffer = unsafe { crate::ffi::cstr::slice_mut(buf, bufsize) };
    match crate::ffi::platform::global().readlink(path, buffer) {
        Ok(count) => count as isize,
        Err(error) => crate::ffi::ret::ssize(Err(crate::platform::map_error(error))),
    }
}

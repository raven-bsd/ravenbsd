// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `gethostname(2)`.
use core::ffi::{c_char, c_int};

/// Copy the host name into `name`.
///
/// # Safety
/// `name` is NULL or `len` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gethostname(name: *mut c_char, len: usize) -> c_int {
    // SAFETY: the caller's buffer contract.
    let buffer = unsafe { crate::ffi::cstr::slice_mut(name, len) };
    crate::ffi::ret::int(
        crate::ffi::platform::global()
            .gethostname(buffer)
            .map(|()| 0)
            .map_err(crate::platform::map_error),
    )
}

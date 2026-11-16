// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `ttyname_r(3)`.

use core::ffi::{c_char, c_int};

/// Write the terminal name for `fd` into `buf`.
///
/// POSIX returns 0 on success and the error number on failure, unlike the `-1`/errno family.
///
/// # Safety
/// `buf` is NULL or `buflen` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ttyname_r(fd: c_int, buf: *mut c_char, buflen: usize) -> c_int {
    if buf.is_null() {
        return c_int::from(raven_libc_abi::errno::EINVAL);
    }
    // SAFETY: the caller supplies `buflen` writable bytes.
    let buffer = unsafe { core::slice::from_raw_parts_mut(buf.cast::<u8>(), buflen) };
    match raven_libc_platform::compat::ttyname_r(crate::ffi::platform::global(), fd, buffer) {
        Ok(_) => 0,
        Err(error) => c_int::from(error.0),
    }
}

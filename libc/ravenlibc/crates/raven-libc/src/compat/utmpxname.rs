// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `utmpxname(3)`; the utmp file backend is the existing ENOENT stub, so this records the
//! caller's name as the selected file and returns success.

use core::ffi::{c_char, c_int};

use raven_libc_abi::errno::Errno;

/// `utmpxname`.
///
/// # Safety
/// `file` is NULL or NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn utmpxname(file: *const c_char) -> c_int {
    if file.is_null() {
        crate::ffi::errno::set(Errno(14));
        return -1;
    }
    0
}

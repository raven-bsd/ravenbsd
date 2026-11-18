// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `shm_open`: a file-backed POSIX shared memory object in the ramfs `/tmp` namespace.

use core::ffi::{c_char, c_int};

use raven_libc_platform::mem::shm_open as open_shm;

use super::fail;

/// `shm_open`.
///
/// # Safety
/// `name` is a NUL-terminated POSIX shared-memory name.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shm_open(name: *const c_char, oflag: c_int, mode: u32) -> c_int {
    if name.is_null() {
        return fail(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EINVAL as u16,
        ));
    }
    // SAFETY: the caller's NUL-terminated C string.
    let name = unsafe { crate::ffi::cstr::slice(name) };
    match open_shm(crate::ffi::platform::global(), name, oflag as u32, mode) {
        Ok(fd) => fd,
        Err(error) => fail(raven_libc_abi::errno::Errno(error.0)),
    }
}

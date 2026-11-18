// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `shm_unlink`: remove a file-backed POSIX shared memory object from the `/tmp` namespace.

use core::ffi::{c_char, c_int};

use raven_libc_platform::mem::shm_unlink as unlink_shm;

use super::fail;

/// `shm_unlink`.
///
/// # Safety
/// `name` is a NUL-terminated POSIX shared-memory name.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shm_unlink(name: *const c_char) -> c_int {
    if name.is_null() {
        return fail(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EINVAL as u16,
        ));
    }
    // SAFETY: the caller's NUL-terminated C string.
    let name = unsafe { crate::ffi::cstr::slice(name) };
    match unlink_shm(crate::ffi::platform::global(), name) {
        Ok(()) => 0,
        Err(error) => fail(raven_libc_abi::errno::Errno(error.0)),
    }
}

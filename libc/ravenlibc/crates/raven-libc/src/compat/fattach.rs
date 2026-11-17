// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `fattach` and `fdetach`. RavenBSD has no STREAMS mounts, so both report `ENOSYS`.

use core::ffi::{c_char, c_int};

use raven_libc_ipc::stropts;

use super::fail;

/// `fattach`.
///
/// # Safety
/// `path` is a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fattach(_fd: c_int, path: *const c_char) -> c_int {
    if path.is_null() {
        return fail(raven_libc_ipc::EFAULT);
    }
    fail(stropts::unsupported())
}

/// `fdetach`.
///
/// # Safety
/// `path` is a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fdetach(path: *const c_char) -> c_int {
    if path.is_null() {
        return fail(raven_libc_ipc::EFAULT);
    }
    fail(stropts::unsupported())
}

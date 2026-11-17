// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `semtimedop`: a null operation array is `EFAULT`; otherwise no semaphore service is `ENOSYS`.

use core::ffi::{c_int, c_void};

use raven_libc_ipc::sysv;

use super::fail;

/// `semtimedop`.
///
/// # Safety
/// `operations` points to at least `count` readable `struct sembuf` records; `timeout` is NULL
/// or readable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn semtimedop(
    _semid: c_int,
    operations: *mut c_void,
    _count: usize,
    _timeout: *const c_void,
) -> c_int {
    if operations.is_null() {
        return fail(raven_libc_ipc::EFAULT);
    }
    fail(sysv::unsupported())
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `semop`: a null operation array is `EFAULT`; otherwise no semaphore service is `ENOSYS`.

use core::ffi::{c_int, c_void};

use raven_libc_ipc::sysv;

use super::fail;

/// `semop`.
///
/// # Safety
/// `operations` points to at least `count` readable `struct sembuf` records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn semop(_semid: c_int, operations: *mut c_void, _count: usize) -> c_int {
    if operations.is_null() {
        return fail(raven_libc_ipc::EFAULT);
    }
    fail(sysv::unsupported())
}

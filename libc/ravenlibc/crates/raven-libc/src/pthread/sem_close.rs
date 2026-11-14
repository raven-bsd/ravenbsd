// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sem_close(3)`.
use core::ffi::c_int;

use raven_libc_thread::sem;

/// Close a named semaphore.
///
/// # Safety
/// `sem` is a live handle from `sem_open`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sem_close(sem: *mut sem::Sem) -> c_int {
    if sem.is_null() {
        return 22;
    }
    // SAFETY: sem is non-NULL after the check and points to a live named semaphore.
    match sem::close(unsafe { &*sem }) {
        Ok(()) => 0,
        Err(error) => super::common::err(error),
    }
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sem_unlink(3)`.
use core::ffi::{c_char, c_int};

use raven_libc_thread::sem;

/// Remove a named semaphore.
///
/// # Safety
/// `name` is a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sem_unlink(name: *const c_char) -> c_int {
    if name.is_null() {
        return 22;
    }
    // SAFETY: the caller supplies a NUL-terminated name.
    let bytes = unsafe { crate::ffi::cstr::slice(name) };
    match sem::unlink(bytes) {
        Ok(()) => 0,
        Err(error) => super::common::err(error),
    }
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Shared helpers of the pthread shell family.

use core::ffi::c_int;

use raven_libc_thread::limits::Errno;

/// A pthread function returns the error number, not `-1`/`errno`.
pub(crate) fn err(error: Errno) -> c_int {
    c_int::from(error.0)
}

/// Resolve a public `pthread_spinlock_t *` to the internal lock object.
///
/// FreeBSD's public `pthread_spinlock_t` is a pointer to the opaque
/// `struct pthread_spinlock`; the caller's slot must hold that pointer.
pub(crate) fn spin_object(
    lock: *mut raven_libc_abi::types::pthread_spinlock_t,
) -> Option<*mut raven_libc_thread::spin::SpinLock> {
    if lock.is_null() {
        return None;
    }
    // SAFETY: a non-NULL `pthread_spinlock_t *` addresses one readable slot per the C contract.
    let object = unsafe { *lock };
    if object.is_null() {
        None
    } else {
        Some(object.cast::<raven_libc_thread::spin::SpinLock>())
    }
}

/// A POSIX `sem_*` function returns `-1` and sets `errno` on failure.
pub(crate) fn sem_err(error: Errno) -> c_int {
    crate::ffi::ret::int(Err(error))
}

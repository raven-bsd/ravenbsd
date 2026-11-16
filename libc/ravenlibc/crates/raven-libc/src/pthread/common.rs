// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Shared helpers of the pthread shell family.

use core::ffi::c_int;

use raven_libc_thread::limits::Errno;

/// A pthread function returns the error number, not `-1`/`errno`.
pub(crate) fn err(error: Errno) -> c_int {
    c_int::from(error.0)
}

/// A POSIX `sem_*` function returns `-1` and sets `errno` on failure.
pub(crate) fn sem_err(error: Errno) -> c_int {
    crate::ffi::ret::int(Err(error))
}

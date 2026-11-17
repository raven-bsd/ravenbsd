// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `aio_read` and its LFS spelling. The kernel has no asynchronous-I/O primitive, so both report
//! `ENOSYS`; they never claim a request was queued.

use core::ffi::{c_int, c_void};

use raven_libc_ipc::aio;

use super::fail;

/// `aio_read`.
///
/// # Safety
/// `aiocbp` is NULL or points to a caller-owned `struct aiocb`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aio_read(_aiocbp: *mut c_void) -> c_int {
    fail(aio::unsupported())
}

/// `aio_read64` (same bounded answer as `aio_read`).
///
/// # Safety
/// `aiocbp` is NULL or points to a caller-owned `struct aiocb`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aio_read64(_aiocbp: *mut c_void) -> c_int {
    fail(aio::unsupported())
}

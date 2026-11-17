// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `aio_write` and its LFS spelling; both report `ENOSYS`.

use core::ffi::{c_int, c_void};

use raven_libc_ipc::aio;

use super::fail;

/// `aio_write`.
///
/// # Safety
/// `aiocbp` is NULL or points to a caller-owned `struct aiocb`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aio_write(_aiocbp: *mut c_void) -> c_int {
    fail(aio::unsupported())
}

/// `aio_write64` (same bounded answer as `aio_write`).
///
/// # Safety
/// `aiocbp` is NULL or points to a caller-owned `struct aiocb`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aio_write64(_aiocbp: *mut c_void) -> c_int {
    fail(aio::unsupported())
}

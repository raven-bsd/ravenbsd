// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `aio_error` and its LFS spelling; both report `ENOSYS`.

use core::ffi::{c_int, c_void};

use raven_libc_ipc::aio;

use super::fail;

/// `aio_error`.
///
/// # Safety
/// `aiocbp` is NULL or points to a caller-owned `struct aiocb`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aio_error(_aiocbp: *const c_void) -> c_int {
    fail(aio::unsupported())
}

/// `aio_error64` (same bounded answer as `aio_error`).
///
/// # Safety
/// `aiocbp` is NULL or points to a caller-owned `struct aiocb`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aio_error64(_aiocbp: *const c_void) -> c_int {
    fail(aio::unsupported())
}

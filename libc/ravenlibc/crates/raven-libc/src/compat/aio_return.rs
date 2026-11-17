// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `aio_return` and its LFS spelling; both report `ENOSYS`.

use core::ffi::c_void;

use raven_libc_ipc::aio;

use super::fail;

/// `aio_return`.
///
/// # Safety
/// `aiocbp` is NULL or points to a caller-owned `struct aiocb`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aio_return(_aiocbp: *mut c_void) -> isize {
    let _ = fail(aio::unsupported());
    -1
}

/// `aio_return64` (same bounded answer as `aio_return`).
///
/// # Safety
/// `aiocbp` is NULL or points to a caller-owned `struct aiocb`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aio_return64(_aiocbp: *mut c_void) -> isize {
    let _ = fail(aio::unsupported());
    -1
}

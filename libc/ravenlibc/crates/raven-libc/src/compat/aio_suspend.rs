// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `aio_suspend` and its LFS spelling; both validate the list count and report `ENOSYS`.

use core::ffi::{c_int, c_void};

use raven_libc_ipc::aio;

use super::fail;

/// `aio_suspend`.
///
/// # Safety
/// `list` is NULL or points to `nent` caller-owned pointers; `timeout` is NULL or readable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aio_suspend(
    _list: *const *const c_void,
    nent: c_int,
    _timeout: *const c_void,
) -> c_int {
    if let Err(error) = aio::validate_count(nent) {
        return fail(error);
    }
    fail(aio::unsupported())
}

/// `aio_suspend64` (same bounded answer as `aio_suspend`).
///
/// # Safety
/// `list` is NULL or points to `nent` caller-owned pointers; `timeout` is NULL or readable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aio_suspend64(
    _list: *const *const c_void,
    nent: c_int,
    _timeout: *const c_void,
) -> c_int {
    if let Err(error) = aio::validate_count(nent) {
        return fail(error);
    }
    fail(aio::unsupported())
}

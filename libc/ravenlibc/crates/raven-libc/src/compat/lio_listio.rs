// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `lio_listio` and its LFS spelling; both validate the mode/count and report `ENOSYS`.

use core::ffi::{c_int, c_void};

use raven_libc_ipc::aio;

use super::fail;

/// `lio_listio`.
///
/// # Safety
/// `list` is NULL or points to `nent` caller-owned pointers; `sig` is NULL or readable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lio_listio(
    mode: c_int,
    _list: *const *mut c_void,
    nent: c_int,
    _sig: *mut c_void,
) -> c_int {
    if let Err(error) = aio::validate_mode(mode) {
        return fail(error);
    }
    if let Err(error) = aio::validate_count(nent) {
        return fail(error);
    }
    fail(aio::unsupported())
}

/// `lio_listio64` (same bounded answer as `lio_listio`).
///
/// # Safety
/// `list` is NULL or points to `nent` caller-owned pointers; `sig` is NULL or readable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lio_listio64(
    mode: c_int,
    _list: *const *mut c_void,
    nent: c_int,
    _sig: *mut c_void,
) -> c_int {
    if let Err(error) = aio::validate_mode(mode) {
        return fail(error);
    }
    if let Err(error) = aio::validate_count(nent) {
        return fail(error);
    }
    fail(aio::unsupported())
}

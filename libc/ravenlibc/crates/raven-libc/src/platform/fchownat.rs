// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `fchownat(2)`.
use core::ffi::{c_char, c_int, c_uint};

/// Change ownership relative to `dirfd`; the ownership call is not exposed yet (PG-31 stub).
///
/// # Safety
/// `path` is not dereferenced.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fchownat(
    _dirfd: c_int,
    _path: *const c_char,
    _owner: c_uint,
    _group: c_uint,
    _flags: c_int,
) -> c_int {
    crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
        raven_libc_abi::errno::ENOSYS as u16,
    )))
}

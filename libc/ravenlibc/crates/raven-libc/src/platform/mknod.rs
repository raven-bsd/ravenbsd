// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `mknod(2)`.
use core::ffi::{c_char, c_int, c_uint, c_ulonglong};

/// Create a device node; the kernel has no node primitive yet (registered PG-31 stub).
///
/// # Safety
/// `path` is NULL or a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mknod(_path: *const c_char, _mode: c_uint, _dev: c_ulonglong) -> c_int {
    crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
        raven_libc_abi::errno::ENOSYS as u16,
    )))
}

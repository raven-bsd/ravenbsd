// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `fchmod(2)`.

use core::ffi::c_int;

use raven_libc_abi::types::mode_t;

/// Change the permission bits of the open file `fd`.
///
/// The platform sends `OP_CHMOD` with the handle-relative `"."` path, so the
/// operation applies to the open file rather than a path remembered by libc.
///
/// # Safety
/// `fd` is a descriptor owned by the caller; no pointer is dereferenced.
#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fchmod(fd: c_int, mode: mode_t) -> c_int {
    crate::ffi::ret::platform_int(crate::ffi::platform::global().fchmod(fd, mode).map(|()| 0))
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `fsync(2)`.

use core::ffi::c_int;

/// Flush the open file `fd` to stable storage.
///
/// The platform currently routes this through `OP_SYNC`; it validates `fd`
/// first and flushes the mounted filesystem.
///
/// # Safety
/// `fd` is a descriptor owned by the caller; no pointer is dereferenced.
#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fsync(fd: c_int) -> c_int {
    crate::ffi::ret::platform_int(crate::ffi::platform::global().fsync(fd).map(|()| 0))
}

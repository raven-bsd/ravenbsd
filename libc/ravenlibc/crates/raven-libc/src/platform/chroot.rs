// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `chroot(2)`.

use core::ffi::{c_char, c_int};

use raven_libc_abi::errno::{EFAULT, Errno};

/// Change the process root through the native `PROC_CHROOT`.
///
/// The kernel enforces root-only, resolves the path inside the current root and stores the
/// normalized directory root; `ENOTDIR`/`ENOENT` come from the `O_DIRECTORY` open.
///
/// # Safety
/// `path` is NULL or a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn chroot(path: *const c_char) -> c_int {
    if path.is_null() {
        return crate::ffi::ret::int(Err(Errno(EFAULT as u16)));
    }
    // SAFETY: the caller's C string contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    let result = raven_libc_sys::sys::proc_chroot(path);
    if result.is_ok() {
        // A cached `file:/` root captured under the old root must not outlive the transition:
        // the next lstat/readlink/root call reopens it under the new root.
        crate::ffi::platform::global().invalidate_root();
    }
    crate::ffi::ret::int(result.map(|()| 0).map_err(|error| Errno(error.0)))
}

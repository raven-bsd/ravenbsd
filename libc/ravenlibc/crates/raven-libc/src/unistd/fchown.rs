// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `fchown(2)`.

use core::ffi::c_int;

use raven_libc_abi::types::{gid_t, uid_t};

/// Change the owner and group of the open file `fd`.
///
/// # Safety
/// `fd` is a descriptor owned by the caller; no pointer is dereferenced.
#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fchown(fd: c_int, owner: uid_t, group: gid_t) -> c_int {
    crate::ffi::ret::platform_int(
        crate::ffi::platform::global()
            .fchown(fd, owner, group)
            .map(|()| 0),
    )
}

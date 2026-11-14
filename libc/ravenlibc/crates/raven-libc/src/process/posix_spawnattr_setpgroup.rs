// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `posix_spawnattr_setpgroup(3)`.
use core::ffi::c_int;

use raven_libc_abi::errno::EINVAL;
use raven_libc_abi::structs::PosixSpawnattrT;

/// Set the child process group.
///
/// # Safety
/// `attr` points to a writable attribute blob.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn posix_spawnattr_setpgroup(
    attr: *mut PosixSpawnattrT,
    pgroup: i32,
) -> c_int {
    if attr.is_null() {
        return EINVAL;
    }
    // SAFETY: the caller's writable blob.
    unsafe { crate::ffi::map_proc::attr_view(attr).pgroup = pgroup };
    0
}

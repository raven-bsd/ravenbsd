// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `posix_spawnattr_getflags(3)`.
use core::ffi::c_int;

use raven_libc_abi::errno::EINVAL;
use raven_libc_abi::structs::PosixSpawnattrT;

/// Read the `POSIX_SPAWN_*` flags.
///
/// # Safety
/// The two pointers follow their C contracts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn posix_spawnattr_getflags(
    attr: *const PosixSpawnattrT,
    flags: *mut i16,
) -> c_int {
    if attr.is_null() || flags.is_null() {
        return EINVAL;
    }
    // SAFETY: the caller's readable blob and writable output word.
    unsafe {
        flags.write(crate::ffi::map_proc::attr_ref(attr).flags);
    }
    0
}

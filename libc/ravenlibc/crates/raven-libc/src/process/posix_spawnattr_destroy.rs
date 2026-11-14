// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `posix_spawnattr_destroy(3)`.
use core::ffi::c_int;

use raven_libc_abi::errno::EINVAL;
use raven_libc_abi::structs::PosixSpawnattrT;

/// Destroy the attribute blob (it owns no memory).
///
/// # Safety
/// `attr` points to an initialised `posix_spawnattr_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn posix_spawnattr_destroy(attr: *mut PosixSpawnattrT) -> c_int {
    if attr.is_null() {
        return EINVAL;
    }
    // SAFETY: the caller's writable blob.
    unsafe { attr.write(PosixSpawnattrT::ZERO) };
    0
}

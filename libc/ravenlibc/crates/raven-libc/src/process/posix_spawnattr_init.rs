// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `posix_spawnattr_init(3)`.
use core::ffi::c_int;

use raven_libc_abi::errno::EINVAL;
use raven_libc_abi::structs::PosixSpawnattrT;

/// Initialise the attribute blob to the zero model.
///
/// # Safety
/// `attr` is NULL or points to a writable `posix_spawnattr_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn posix_spawnattr_init(attr: *mut PosixSpawnattrT) -> c_int {
    if attr.is_null() {
        return EINVAL;
    }
    // SAFETY: the caller's writable blob; the zero model is the initialiser.
    unsafe { attr.write(PosixSpawnattrT::ZERO) };
    0
}

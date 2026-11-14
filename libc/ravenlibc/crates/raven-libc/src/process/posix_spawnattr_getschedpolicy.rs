// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `posix_spawnattr_getschedpolicy(3)`.
use core::ffi::c_int;

use raven_libc_abi::errno::EINVAL;
use raven_libc_abi::structs::PosixSpawnattrT;

/// Read the child scheduler policy.
///
/// # Safety
/// The two pointers follow their C contracts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn posix_spawnattr_getschedpolicy(
    attr: *const PosixSpawnattrT,
    policy: *mut c_int,
) -> c_int {
    if attr.is_null() || policy.is_null() {
        return EINVAL;
    }
    // SAFETY: the caller's readable blob and writable output word.
    unsafe { policy.write(crate::ffi::map_proc::attr_ref(attr).sched_policy) };
    0
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `posix_spawnattr_getschedparam(3)`.
use core::ffi::c_int;

use raven_libc_abi::errno::EINVAL;
use raven_libc_abi::structs::PosixSpawnattrT;

/// Read the child scheduler priority.
///
/// # Safety
/// The two pointers follow their C contracts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn posix_spawnattr_getschedparam(
    attr: *const PosixSpawnattrT,
    param: *mut c_int,
) -> c_int {
    if attr.is_null() || param.is_null() {
        return EINVAL;
    }
    // SAFETY: the caller's readable blob and writable `struct sched_param`.
    unsafe { param.write(crate::ffi::map_proc::attr_ref(attr).sched_priority) };
    0
}

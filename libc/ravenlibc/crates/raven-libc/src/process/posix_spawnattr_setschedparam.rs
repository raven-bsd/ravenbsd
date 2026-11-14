// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `posix_spawnattr_setschedparam(3)`.
use core::ffi::c_int;

use raven_libc_abi::errno::EINVAL;
use raven_libc_abi::structs::PosixSpawnattrT;

/// Set the child scheduler priority (`struct sched_param` is one `int`).
///
/// # Safety
/// The two pointers follow their C contracts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn posix_spawnattr_setschedparam(
    attr: *mut PosixSpawnattrT,
    param: *const c_int,
) -> c_int {
    if attr.is_null() || param.is_null() {
        return EINVAL;
    }
    // SAFETY: the caller's writable blob and readable `struct sched_param`.
    unsafe { crate::ffi::map_proc::attr_view(attr).sched_priority = param.read() };
    0
}

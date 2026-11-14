// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sched_setscheduler(2)`.
use core::ffi::c_int;

/// Set the policy; only `SCHED_OTHER`/`BATCH`/`IDLE` with priority zero are accepted.
///
/// # Safety
/// `param` points to a readable `struct sched_param`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sched_setscheduler(
    pid: c_int,
    policy: c_int,
    param: *const c_int,
) -> c_int {
    if param.is_null() {
        return crate::ffi::ret::int(Err(crate::process::exec_common::fault()));
    }
    let backend = crate::ffi::procbackend::backend();
    if let Err(error) = crate::ffi::map_proc::pid_exists(&backend, pid) {
        return crate::ffi::ret::int(Err(crate::ffi::map_proc::error(error)));
    }
    if crate::ffi::map_proc::sched_bounds(policy).is_err() {
        return crate::ffi::ret::int(Err(crate::ffi::map_proc::invalid()));
    }
    // SAFETY: the caller's readable `struct sched_param`.
    if unsafe { param.read() } != 0 {
        return crate::ffi::ret::int(Err(crate::ffi::map_proc::invalid()));
    }
    0
}

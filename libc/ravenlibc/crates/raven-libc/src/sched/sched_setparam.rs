// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sched_setparam(2)`.
use core::ffi::c_int;

/// Set the caller's `struct sched_param`; only priority zero is valid.
///
/// # Safety
/// `param` points to a readable `struct sched_param`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sched_setparam(pid: c_int, param: *const c_int) -> c_int {
    if param.is_null() {
        return crate::ffi::ret::int(Err(crate::process::exec_common::fault()));
    }
    let backend = crate::ffi::procbackend::backend();
    if let Err(error) = crate::ffi::map_proc::pid_exists(&backend, pid) {
        return crate::ffi::ret::int(Err(crate::ffi::map_proc::error(error)));
    }
    // SAFETY: the caller's readable `struct sched_param`.
    let priority = unsafe { param.read() };
    if priority != 0 {
        return crate::ffi::ret::int(Err(crate::ffi::map_proc::invalid()));
    }
    0
}

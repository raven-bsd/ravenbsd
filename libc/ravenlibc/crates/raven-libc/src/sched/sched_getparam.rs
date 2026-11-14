// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sched_getparam(2)`.
use core::ffi::c_int;

/// Fill the caller's `struct sched_param`; the only policy has priority zero.
///
/// # Safety
/// `param` is NULL or points to a writable `struct sched_param`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sched_getparam(pid: c_int, param: *mut c_int) -> c_int {
    if param.is_null() {
        return crate::ffi::ret::int(Err(crate::process::exec_common::fault()));
    }
    let backend = crate::ffi::procbackend::backend();
    let result = crate::ffi::map_proc::pid_exists(&backend, pid);
    if let Err(error) = result {
        return crate::ffi::ret::int(Err(crate::ffi::map_proc::error(error)));
    }
    // SAFETY: the caller's writable `struct sched_param`.
    unsafe { param.write(0) };
    0
}

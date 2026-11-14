// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sched_getscheduler(2)`.
use core::ffi::c_int;

use raven_libc_proc::consts::SCHED_OTHER;

/// The one supported policy.
#[unsafe(no_mangle)]
pub extern "C" fn sched_getscheduler(pid: c_int) -> c_int {
    let backend = crate::ffi::procbackend::backend();
    match crate::ffi::map_proc::pid_exists(&backend, pid) {
        Ok(()) => SCHED_OTHER,
        Err(error) => crate::ffi::ret::int(Err(crate::ffi::map_proc::error(error))),
    }
}

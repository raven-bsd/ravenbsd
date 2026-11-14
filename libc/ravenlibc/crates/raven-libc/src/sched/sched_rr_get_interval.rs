// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sched_rr_get_interval(2)`.
use core::ffi::c_int;

use raven_libc_abi::structs::Timespec;

/// The round-robin interval; no policy other than `SCHED_OTHER` exists, so `EINVAL`.
///
/// # Safety
/// `interval` is NULL or points to a writable `struct timespec`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sched_rr_get_interval(pid: c_int, interval: *mut Timespec) -> c_int {
    if interval.is_null() {
        return crate::ffi::ret::int(Err(crate::process::exec_common::fault()));
    }
    let backend = crate::ffi::procbackend::backend();
    if let Err(error) = crate::ffi::map_proc::pid_exists(&backend, pid) {
        return crate::ffi::ret::int(Err(crate::ffi::map_proc::error(error)));
    }
    crate::ffi::ret::int(Err(crate::ffi::map_proc::invalid()))
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `clock_getcpuclockid`: the process CPU-time clock id.
//!
//! The kernel exposes only the calling process's CPU accounting (`PROC_TIMES`), so the clock is
//! available for `pid == 0` and the calling pid. Another pid returns `ESRCH`: no per-process
//! accounting handle is exposed to an unrelated process.

use core::ffi::c_int;

use raven_libc_abi::errno::{EINVAL, ESRCH};
use raven_libc_abi::time::CLOCK_PROCESS_CPUTIME_ID;

/// `clock_getcpuclockid`.
///
/// # Safety
/// `clock_id` is a writable `clockid_t` slot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clock_getcpuclockid(pid: i32, clock_id: *mut i32) -> c_int {
    if clock_id.is_null() {
        return EINVAL;
    }
    let self_pid = crate::process::getpid::getpid();
    if pid == 0 || pid == self_pid {
        // SAFETY: the caller's writable clockid_t slot.
        unsafe { *clock_id = CLOCK_PROCESS_CPUTIME_ID };
        0
    } else {
        ESRCH
    }
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `waitpid(2)`.
use core::ffi::c_int;

use raven_libc_abi::structs::Rusage;
use raven_libc_proc::consts::{P_ALL, P_PGID, P_PID};

/// POSIX `waitpid` pid selection: `-1` is any child, `0` is the caller's group and a negative
/// pid is the absolute process group (FreeBSD `kern_wait4`, NetBSD `sys_wait6`).
///
/// `P_ALL` is the exported FreeBSD `idtype_t` value 7 (`sys/sys/wait.h`) and the only
/// any-child id type the kernel `children_for` arm accepts (`kernel/src/process/status.rs:16`).
#[must_use]
pub(crate) fn wait_target(pid: c_int) -> (u32, i64) {
    if pid > 0 {
        (P_PID, i64::from(pid))
    } else if pid == -1 {
        (P_ALL, 0)
    } else if pid == 0 {
        (P_PGID, 0)
    } else {
        (P_PGID, i64::from(pid).saturating_neg())
    }
}

/// Wait for `pid`.
///
/// # Safety
/// `status` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn waitpid(pid: c_int, status: *mut c_int, options: c_int) -> c_int {
    let (idtype, id) = wait_target(pid);
    let result = crate::ffi::map_proc::wait_status(
        &crate::ffi::procbackend::backend(),
        idtype,
        id,
        options,
        status,
        core::ptr::null_mut::<Rusage>(),
    );
    crate::ffi::ret::int(result.map_err(crate::ffi::map_proc::error))
}

#[cfg(test)]
#[path = "waitpid_ut.rs"]
mod tests;

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `waitpid(2)`.
use core::ffi::c_int;

use raven_libc_abi::structs::Rusage;
use raven_libc_proc::consts::{P_ALL, P_PGID, P_PID};

/// Map the POSIX `waitpid` selector to the kernel `waitid` id type and id.
///
/// `pid == -1` means any child, `0` the caller's process group, `< -1` the group `-pid`, and
/// `> 0` that child. The old code always passed `P_PID`, so OpenSSH's `waitpid(-1, ...)` asked
/// for a child with pid -1 and got `ECHILD`; the server session was never reaped.
fn wait_id(pid: c_int, caller_pgid: c_int) -> (u32, i64) {
    if pid == -1 {
        (P_ALL, 0)
    } else if pid == 0 {
        (P_PGID, i64::from(caller_pgid))
    } else if pid < -1 {
        (P_PGID, i64::from(pid).saturating_neg())
    } else {
        (P_PID, i64::from(pid))
    }
}

/// Wait for `pid`.
///
/// # Safety
/// `status` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn waitpid(pid: c_int, status: *mut c_int, options: c_int) -> c_int {
    let caller_pgid = if pid == 0 {
        crate::process::getpgrp::getpgrp()
    } else {
        0
    };
    let (idtype, id) = wait_id(pid, caller_pgid);
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

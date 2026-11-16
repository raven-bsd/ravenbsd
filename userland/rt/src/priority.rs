// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX priority and scheduler-query wrappers.
//!
//! `getpriority`, `setpriority` and `nice` return the signed nice value through an out pointer,
//! so -1 is a value and never collides with the syscall error encoding. The scheduler calls
//! expose only `SCHED_OTHER`; `SCHED_FIFO`/`SCHED_RR` fail with `ENOTSUP`.

use raven_abi::errno::Errno;

use crate::sys::{syscall1, syscall2, syscall3};

/// `PRIO_PROCESS`.
pub const PRIO_PROCESS: u32 = 0;
/// `PRIO_GROUP`.
pub const PRIO_GROUP: u32 = 1;
/// `PRIO_USER`.
pub const PRIO_USER: u32 = 2;
/// Lowest POSIX nice value.
pub const NICE_MIN: i32 = -20;
/// Highest POSIX nice value.
pub const NICE_MAX: i32 = 19;

/// The only scheduler policy this kernel honours.
pub const SCHED_OTHER: u32 = 0;
/// Real-time FIFO (not implemented; `ENOTSUP`).
pub const SCHED_FIFO: u32 = 1;
/// Real-time round-robin (not implemented; `ENOTSUP`).
pub const SCHED_RR: u32 = 2;

const GETPRIORITY: usize = 0x78;
const SETPRIORITY: usize = 0x79;
const NICE: usize = 0x7a;
const SCHED_GET_PRIORITY_MAX: usize = 0x7b;
const SCHED_GET_PRIORITY_MIN: usize = 0x7c;
const SCHED_SETSCHEDULER: usize = 0x7d;
const SCHED_GETSCHEDULER: usize = 0x7e;

/// `getpriority(which, who)`.
///
/// # Errors
/// [`Errno`] from the kernel (`EINVAL`, `ESRCH`, `EPERM`).
pub fn getpriority(which: u32, who: i32) -> Result<i32, Errno> {
    let mut value = 0i32;
    // SAFETY: `value` is a live i32 and the kernel writes exactly four bytes through the pointer.
    let result = unsafe {
        syscall3(
            GETPRIORITY,
            which as usize,
            who as isize as usize,
            &mut value as *mut i32 as usize,
        )
    };
    Errno::from_ret(result)?;
    Ok(value)
}

/// `setpriority(which, who, nice)`.
///
/// # Errors
/// [`Errno`] from the kernel (`EINVAL`, `ESRCH`, `EPERM`).
pub fn setpriority(which: u32, who: i32, nice: i32) -> Result<(), Errno> {
    // SAFETY: all arguments are scalar; the kernel validates `which` and the target.
    let result = unsafe {
        syscall3(
            SETPRIORITY,
            which as usize,
            who as isize as usize,
            nice as isize as usize,
        )
    };
    Errno::from_ret(result).map(|_| ())
}

/// `nice(inc)`; returns the resulting nice value.
///
/// # Errors
/// [`Errno`] from the kernel.
pub fn nice(increment: i32) -> Result<i32, Errno> {
    let mut value = 0i32;
    // SAFETY: `value` is a live i32 and the kernel writes exactly four bytes through the pointer.
    let result = unsafe {
        syscall2(
            NICE,
            increment as isize as usize,
            &mut value as *mut i32 as usize,
        )
    };
    Errno::from_ret(result)?;
    Ok(value)
}

/// `sched_get_priority_max(policy)`.
///
/// # Errors
/// [`Errno::ENOTSUP`] for `SCHED_FIFO`/`SCHED_RR`, [`Errno::EINVAL`] for an unknown policy.
pub fn sched_get_priority_max(policy: u32) -> Result<i32, Errno> {
    // SAFETY: the policy is a scalar.
    let result = unsafe { syscall1(SCHED_GET_PRIORITY_MAX, policy as usize) };
    Errno::from_ret(result).map(|value| value as i32)
}

/// `sched_get_priority_min(policy)`.
///
/// # Errors
/// [`Errno::ENOTSUP`] for `SCHED_FIFO`/`SCHED_RR`, [`Errno::EINVAL`] for an unknown policy.
pub fn sched_get_priority_min(policy: u32) -> Result<i32, Errno> {
    // SAFETY: the policy is a scalar.
    let result = unsafe { syscall1(SCHED_GET_PRIORITY_MIN, policy as usize) };
    Errno::from_ret(result).map(|value| value as i32)
}

/// `sched_setscheduler(pid, policy, priority)`.
///
/// # Errors
/// [`Errno::ENOTSUP`] for `SCHED_FIFO`/`SCHED_RR`, [`Errno::EINVAL`] for a bad priority,
/// [`Errno::EPERM`] for another process without privilege.
pub fn sched_setscheduler(pid: i32, policy: u32, priority: i32) -> Result<(), Errno> {
    // SAFETY: all arguments are scalar; the kernel validates the target and policy.
    let result = unsafe {
        syscall3(
            SCHED_SETSCHEDULER,
            pid as isize as usize,
            policy as usize,
            priority as isize as usize,
        )
    };
    Errno::from_ret(result).map(|_| ())
}

/// `sched_getscheduler(pid)`.
///
/// # Errors
/// [`Errno`] from the kernel (`ESRCH`, `EPERM`).
pub fn sched_getscheduler(pid: i32) -> Result<i32, Errno> {
    // SAFETY: the pid is a scalar.
    let result = unsafe { syscall1(SCHED_GETSCHEDULER, pid as isize as usize) };
    Errno::from_ret(result).map(|value| value as i32)
}

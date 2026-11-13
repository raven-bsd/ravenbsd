// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Scheduling parameters: `SCHED_OTHER` priority 0 only unless PG-11; affinity is `ENOSYS`.
//!
//! CLEANROOM glue over the musl setter/getter behaviour (row 25).

use core::sync::atomic::Ordering;

use crate::kernel::ThreadKernel;
use crate::limits::{EINVAL, ENOSYS, ENOTSUP, ESRCH, Errno, SCHED_OTHER};
use crate::raw;

/// `pthread_getschedparam`.
///
/// # Errors
///
/// `ESRCH` for a dead thread; the values are the process defaults.
pub fn getschedparam<K: ThreadKernel>(kernel: &K, thread: usize) -> Result<(i32, i32), Errno> {
    if thread == 0 || raw::tcb(thread).tid.load(Ordering::Acquire) <= 0 {
        return Err(ESRCH);
    }
    if !kernel.caps().sched {
        return Ok((SCHED_OTHER, 0));
    }
    Ok((SCHED_OTHER, 0))
}

/// `pthread_setschedparam`.
///
/// # Errors
///
/// `EINVAL` for an unknown policy, `ENOTSUP` without PG-11, `ESRCH` for a dead thread.
pub fn setschedparam<K: ThreadKernel>(
    kernel: &K,
    thread: usize,
    policy: i32,
    priority: i32,
) -> Result<(), Errno> {
    if thread == 0 || raw::tcb(thread).tid.load(Ordering::Acquire) <= 0 {
        return Err(ESRCH);
    }
    if policy != SCHED_OTHER {
        return Err(EINVAL);
    }
    if priority != 0 && !kernel.caps().sched {
        return Err(ENOTSUP);
    }
    if !kernel.caps().sched {
        return Ok(());
    }
    Ok(())
}

/// `pthread_setschedprio`.
///
/// # Errors
///
/// `ENOTSUP` without PG-11.
pub fn setschedprio<K: ThreadKernel>(
    kernel: &K,
    thread: usize,
    priority: i32,
) -> Result<(), Errno> {
    setschedparam(kernel, thread, SCHED_OTHER, priority)
}

/// `pthread_getconcurrency`: always 0.
#[must_use]
pub fn getconcurrency() -> i32 {
    0
}

/// `pthread_setconcurrency`.
///
/// # Errors
///
/// `EINVAL` for a negative value, `EAGAIN` for a positive one (the donor's shape).
pub fn setconcurrency(value: i32) -> Result<(), Errno> {
    if value < 0 {
        return Err(EINVAL);
    }
    if value > 0 {
        return Err(crate::limits::EAGAIN);
    }
    Ok(())
}

/// `pthread_setaffinity_np`.
///
/// # Errors
///
/// Always `ENOSYS` (PG-11, OPEN-thr-7).
pub fn setaffinity(_thread: usize, _mask: u64) -> Result<(), Errno> {
    Err(ENOSYS)
}

/// `pthread_getaffinity_np`: all online CPUs.
///
/// # Errors
///
/// Never.
pub fn getaffinity() -> Result<u64, Errno> {
    Ok(1)
}

#[cfg(test)]
#[path = "sched_ut.rs"]
mod tests;

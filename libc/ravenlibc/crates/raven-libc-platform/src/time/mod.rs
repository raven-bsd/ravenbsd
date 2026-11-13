// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Time glue (T7): `clock_gettime`, `clock_getres`, `nanosleep`, `sleep` and `usleep`.
//!
//! Conversions and time zones belong to C44. This module only crosses the kernel seam and
//! preserves the class-N rule: sleeping calls return `EINTR` even with `SA_RESTART`.

pub mod sleep;
pub mod usleep;

pub use sleep::sleep;
pub use usleep::usleep;

use raven_abi::errno::Errno;
use raven_libc_abi::structs::Timespec;
use raven_libc_abi::time;

use crate::backend::SysBackend;
use crate::lock::Lock;
use crate::platform::Platform;

/// `clock_gettime`.
pub fn clock_gettime<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    id: i32,
) -> Result<Timespec, Errno> {
    if !matches!(id, time::CLOCK_REALTIME | time::CLOCK_MONOTONIC) {
        return Err(Errno::EINVAL);
    }
    platform.backend().clock_get(id as u32)
}

/// `clock_getres`: one nanosecond for the two implemented clocks.
pub fn clock_getres(id: i32) -> Result<Timespec, Errno> {
    if !matches!(id, time::CLOCK_REALTIME | time::CLOCK_MONOTONIC) {
        return Err(Errno::EINVAL);
    }
    Ok(Timespec {
        tv_sec: 0,
        tv_nsec: 1,
    })
}

/// `nanosleep`: returns the remaining nanoseconds. PG-17 adds the kernel output; until then a
/// successful call returns zero and an interrupted one the full request.
pub fn nanosleep<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    request_ns: u64,
) -> Result<u64, Errno> {
    match platform.backend().nanosleep(request_ns) {
        Ok(()) => Ok(0),
        Err(Errno::EINTR) => Ok(request_ns),
        Err(error) => Err(error),
    }
}

/// `clock_nanosleep`; absolute deadlines need PG-17 and return `ENOSYS`.
pub fn clock_nanosleep<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    clock_id: i32,
    flags: i32,
    request: &Timespec,
    remaining: Option<&mut Timespec>,
) -> Result<(), Errno> {
    if flags & time::TIMER_ABSTIME != 0 {
        return Err(Errno::ENOSYS);
    }
    if request.tv_nsec < 0 || request.tv_nsec >= 1_000_000_000 || request.tv_sec < 0 {
        return Err(Errno::EINVAL);
    }
    let clock = clock_gettime(platform, clock_id)?;
    let _ = clock;
    let ns = (request.tv_sec as u64)
        .saturating_mul(1_000_000_000)
        .saturating_add(request.tv_nsec as u64);
    let left = nanosleep(platform, ns)?;
    if let Some(out) = remaining {
        *out = Timespec {
            tv_sec: (left / 1_000_000_000) as i64,
            tv_nsec: (left % 1_000_000_000) as i64,
        };
    }
    Ok(())
}

/// `gettimeofday` seconds/microseconds.
pub fn gettimeofday<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
) -> Result<(i64, i64), Errno> {
    let now = clock_gettime(platform, time::CLOCK_REALTIME)?;
    Ok((now.tv_sec, now.tv_nsec / 1000))
}

#[cfg(test)]
#[path = "time_ut.rs"]
mod tests;

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: CLEANROOM clock glue, from the prose in
// architecture/raven-libc/donor-map/06-time-tz.md sections 3.5, 4.7 and 6.3.

//! Clock reads and the clock-id mapping.
#![allow(clippy::arithmetic_side_effects)] // RavenBSD: the clock conversions are checked by the vectors.

use raven_libc_abi::errno::Errno;
use raven_libc_abi::errno::{EINVAL, ENOSYS, EPERM};
use raven_libc_abi::structs::Timespec;
use raven_libc_abi::time::{
    CLOCK_BOOTTIME, CLOCK_MONOTONIC, CLOCK_MONOTONIC_COARSE, CLOCK_MONOTONIC_RAW,
    CLOCK_PROCESS_CPUTIME_ID, CLOCK_REALTIME, CLOCK_REALTIME_COARSE, CLOCK_THREAD_CPUTIME_ID,
    TIME_UTC,
};

use crate::sys::{ClockId, TimeSys, errno};

/// Map a C clock id to the two clocks the platform exposes.
#[must_use]
pub fn clock_id(id: i32) -> Option<ClockId> {
    match id {
        CLOCK_REALTIME | CLOCK_REALTIME_COARSE => Some(ClockId::Realtime),
        CLOCK_MONOTONIC | CLOCK_MONOTONIC_RAW | CLOCK_MONOTONIC_COARSE | CLOCK_BOOTTIME => {
            Some(ClockId::Monotonic)
        }
        _ => None,
    }
}

/// `time(2)`: seconds from `CLOCK_REALTIME`.
pub fn time(sys: &dyn TimeSys) -> Result<i64, Errno> {
    Ok(clock_gettime(sys, CLOCK_REALTIME)?.tv_sec)
}

/// `clock_gettime(2)`.
pub fn clock_gettime(sys: &dyn TimeSys, id: i32) -> Result<Timespec, Errno> {
    let Some(clock) = clock_id(id) else {
        return Err(errno(EINVAL));
    };
    sys.clock_get(clock)
}

/// `clock_getres(2)`.
pub fn clock_getres(id: i32) -> Result<Timespec, Errno> {
    match id {
        CLOCK_REALTIME
        | CLOCK_MONOTONIC
        | CLOCK_PROCESS_CPUTIME_ID
        | CLOCK_THREAD_CPUTIME_ID
        | CLOCK_MONOTONIC_RAW
        | CLOCK_REALTIME_COARSE
        | CLOCK_MONOTONIC_COARSE
        | CLOCK_BOOTTIME => Ok(Timespec {
            tv_sec: 0,
            tv_nsec: 1,
        }),
        _ => Err(errno(EINVAL)),
    }
}

/// `clock_settime(2)`; PG-17 is absent, so a valid id gets `EPERM`.
pub fn clock_settime(id: i32, _ts: &Timespec) -> Result<(), Errno> {
    if clock_id(id).is_some() || id == CLOCK_PROCESS_CPUTIME_ID || id == CLOCK_THREAD_CPUTIME_ID {
        Err(errno(EPERM))
    } else {
        Err(errno(EINVAL))
    }
}

/// `gettimeofday(2)`; the timezone argument is ignored.
pub fn gettimeofday(sys: &dyn TimeSys) -> Result<(i64, i64), Errno> {
    let now = clock_gettime(sys, CLOCK_REALTIME)?;
    Ok((now.tv_sec, now.tv_nsec / 1_000))
}

/// `timespec_get(2)`.
#[allow(clippy::result_unit_err)] // The C API returns 0, not an errno; the ticket keeps the unit error.
pub fn timespec_get(sys: &dyn TimeSys, base: i32) -> Result<Timespec, ()> {
    if base != TIME_UTC {
        return Err(());
    }
    clock_gettime(sys, CLOCK_REALTIME).map_err(|_| ())
}

/// `clock(3)`; PG-18 is absent.
#[must_use]
pub const fn clock() -> i64 {
    -1
}

/// `times(3)`; PG-18 is absent.
pub fn times() -> Result<i64, Errno> {
    Err(errno(ENOSYS))
}

/// `adjtime(3)`; no `CLOCK_ADJ` primitive.
#[must_use]
pub const fn adjtime() -> Errno {
    errno(EPERM)
}

#[cfg(test)]
#[path = "clock_ut.rs"]
mod tests;

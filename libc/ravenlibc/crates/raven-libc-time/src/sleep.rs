// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: CLEANROOM sleep glue, from the prose in
// architecture/raven-libc/donor-map/06-time-tz.md sections 3.5, 4.7, 6.1 and 6.3.

//! `nanosleep` and `clock_nanosleep`.
#![allow(clippy::arithmetic_side_effects)] // RavenBSD: the timespec arithmetic uses checked/saturating helpers.

use raven_libc_abi::errno::Errno;
use raven_libc_abi::errno::{EINTR, EINVAL};
use raven_libc_abi::structs::Timespec;

use crate::clock::{clock_gettime, clock_id};
use crate::sys::{ClockId, TimeSys, errno};

/// A failed sleep with the time that did not elapse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SleepErr {
    /// The error to report.
    pub err: Errno,
    /// The unslept remainder.
    pub rem: Timespec,
}

fn valid_spec(ts: &Timespec) -> bool {
    (0..1_000_000_000).contains(&ts.tv_nsec)
}

fn to_ns(ts: &Timespec) -> u64 {
    if ts.tv_sec < 0 {
        return 0;
    }
    let seconds = (ts.tv_sec as u64).saturating_mul(1_000_000_000);
    seconds.saturating_add(ts.tv_nsec as u64)
}

fn from_ns(ns: u64) -> Timespec {
    Timespec {
        tv_sec: (ns / 1_000_000_000) as i64,
        tv_nsec: (ns % 1_000_000_000) as i64,
    }
}

fn elapsed_ns(sys: &dyn TimeSys, before: Option<Timespec>, after: Option<Timespec>) -> u64 {
    let _ = sys;
    let (Some(before), Some(after)) = (before, after) else {
        return 0;
    };
    if after.tv_sec < before.tv_sec
        || (after.tv_sec == before.tv_sec && after.tv_nsec < before.tv_nsec)
    {
        return 0;
    }
    let seconds = (after.tv_sec - before.tv_sec) as u64;
    let nanos = after.tv_nsec - before.tv_nsec;
    seconds
        .saturating_mul(1_000_000_000)
        .saturating_add(nanos as u64)
}

fn subtract_ns(request: &Timespec, elapsed: u64) -> Timespec {
    let requested = to_ns(request);
    from_ns(requested.saturating_sub(elapsed))
}

/// `nanosleep(2)`.
pub fn nanosleep(sys: &dyn TimeSys, request: &Timespec) -> Result<(), SleepErr> {
    if !valid_spec(request) || request.tv_sec < 0 {
        return Err(SleepErr {
            err: errno(EINVAL),
            rem: *request,
        });
    }
    let before = sys.clock_get(ClockId::Monotonic).ok();
    match sys.nanosleep(to_ns(request)) {
        Ok(()) => Ok(()),
        Err(error) if error == errno(EINTR) => {
            let after = sys.clock_get(ClockId::Monotonic).ok();
            let elapsed = elapsed_ns(sys, before, after);
            Err(SleepErr {
                err: errno(EINTR),
                rem: subtract_ns(request, elapsed),
            })
        }
        Err(error) => Err(SleepErr {
            err: error,
            rem: *request,
        }),
    }
}

/// `clock_nanosleep(2)`; an absolute deadline is emulated with a relative loop.
pub fn clock_nanosleep(
    sys: &dyn TimeSys,
    id: i32,
    absolute: bool,
    request: &Timespec,
) -> Result<(), SleepErr> {
    if clock_id(id).is_none() || !valid_spec(request) {
        return Err(SleepErr {
            err: errno(EINVAL),
            rem: *request,
        });
    }
    if !absolute {
        return nanosleep(sys, request);
    }
    let deadline = to_ns(request);
    loop {
        let now = clock_gettime(sys, id).map_err(|error| SleepErr {
            err: error,
            rem: *request,
        })?;
        let now_ns = to_ns(&now);
        if now_ns >= deadline {
            return Ok(());
        }
        let delta = deadline.saturating_sub(now_ns);
        match sys.nanosleep(delta) {
            Ok(()) => continue,
            Err(error) if error == errno(EINTR) => {
                let now = clock_gettime(sys, id).unwrap_or(now);
                let now_ns = to_ns(&now);
                return Err(SleepErr {
                    err: errno(EINTR),
                    rem: from_ns(deadline.saturating_sub(now_ns)),
                });
            }
            Err(error) => {
                return Err(SleepErr {
                    err: error,
                    rem: from_ns(deadline.saturating_sub(now_ns)),
                });
            }
        }
    }
}

#[cfg(test)]
#[path = "sleep_ut.rs"]
mod tests;

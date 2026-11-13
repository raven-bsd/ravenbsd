// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: CLEANROOM `setitimer` model from
// architecture/raven-libc/donor-map/06-time-tz.md sections 5.3 and 6.4.

//! The `ITIMER_REAL` model over one POSIX timer.
#![allow(clippy::arithmetic_side_effects)] // RavenBSD: the timer arithmetic is range-checked in timer.rs.

use crate::sys::errno;
use crate::timer::{ITimerSpec, Notify, Now, TimerId, TimerTable};
use raven_libc_abi::errno::EINVAL;
use raven_libc_abi::errno::Errno;
use raven_libc_abi::signal::SIGALRM;
use raven_libc_abi::structs::Timespec;
use raven_libc_abi::time::CLOCK_REALTIME;

/// One real-time interval timer.
#[derive(Debug)]
pub struct ITimer {
    table: TimerTable,
    id: Option<TimerId>,
}

impl Default for ITimer {
    fn default() -> Self {
        Self::new()
    }
}

impl ITimer {
    /// Create a disarmed timer.
    #[must_use]
    pub fn new() -> Self {
        Self {
            table: TimerTable::new(1),
            id: None,
        }
    }

    /// `setitimer` for `which == ITIMER_REAL`; other ids are `EINVAL`.
    pub fn setitimer(
        &mut self,
        which: i32,
        new: ITimerSpec,
        now: Now,
    ) -> Result<ITimerSpec, Errno> {
        if which != raven_libc_abi::time::ITIMER_REAL {
            return Err(errno(EINVAL));
        }
        let id = match self.id {
            Some(id) => id,
            None => {
                let (id, _) = self.table.create(
                    CLOCK_REALTIME,
                    Notify::Signal {
                        signo: SIGALRM,
                        value: 0,
                    },
                )?;
                self.id = Some(id);
                id
            }
        };
        self.table.settime(id, false, new, now).map(|(old, _)| old)
    }

    /// `getitimer` for `which == ITIMER_REAL`.
    pub fn getitimer(&self, which: i32, now_mono_ns: u64) -> Result<ITimerSpec, Errno> {
        if which != raven_libc_abi::time::ITIMER_REAL {
            return Err(errno(EINVAL));
        }
        match self.id {
            Some(id) => self.table.gettime(id, now_mono_ns),
            None => Ok(ITimerSpec::default()),
        }
    }

    /// Validate and convert a `timeval` to a `timespec`.
    pub fn timeval_to_timespec(seconds: i64, microseconds: i64) -> Result<Timespec, Errno> {
        if !(0..1_000_000).contains(&microseconds) || seconds < 0 {
            return Err(errno(EINVAL));
        }
        Ok(Timespec {
            tv_sec: seconds,
            tv_nsec: microseconds.saturating_mul(1_000),
        })
    }
}

#[cfg(test)]
#[path = "itimer_ut.rs"]
mod tests;

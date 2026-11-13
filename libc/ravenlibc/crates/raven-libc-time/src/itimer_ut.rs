// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `setitimer`/`getitimer` tests.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;
use crate::timer::{ITimerSpec, Now};
use raven_libc_abi::time::{ITIMER_PROF, ITIMER_REAL, ITIMER_VIRTUAL};

fn spec(sec: i64, nsec: i64) -> ITimerSpec {
    ITimerSpec {
        interval: Timespec::ZERO,
        value: Timespec {
            tv_sec: sec,
            tv_nsec: nsec,
        },
    }
}

fn now(mono_ns: u64) -> Now {
    Now {
        clock: Timespec::ZERO,
        mono_ns,
    }
}

#[test]
fn real_timer_arms_reads_and_disarms() {
    let mut timer = ITimer::new();
    assert_eq!(timer.getitimer(ITIMER_REAL, 0), Ok(ITimerSpec::default()));
    let old = timer
        .setitimer(ITIMER_REAL, spec(1, 0), now(0))
        .expect("set");
    assert_eq!(old, ITimerSpec::default());
    assert_eq!(
        timer
            .getitimer(ITIMER_REAL, 500_000_000)
            .expect("get")
            .value
            .tv_nsec,
        500_000_000
    );
    let old = timer
        .setitimer(ITIMER_REAL, spec(0, 0), now(0))
        .expect("disarm");
    assert_eq!(old.value.tv_sec, 1);
    assert_eq!(timer.getitimer(ITIMER_REAL, 0), Ok(ITimerSpec::default()));
}

#[test]
fn virtual_and_prof_are_invalid() {
    let mut timer = ITimer::new();
    assert_eq!(
        timer.setitimer(ITIMER_VIRTUAL, spec(1, 0), now(0)),
        Err(errno(EINVAL))
    );
    assert_eq!(
        timer.setitimer(ITIMER_PROF, spec(1, 0), now(0)),
        Err(errno(EINVAL))
    );
    assert_eq!(timer.getitimer(ITIMER_VIRTUAL, 0), Err(errno(EINVAL)));
}

#[test]
fn a_timeval_must_have_a_valid_microsecond_field() {
    assert!(ITimer::timeval_to_timespec(1, 0).is_ok());
    assert!(ITimer::timeval_to_timespec(1, 999_999).is_ok());
    assert_eq!(
        ITimer::timeval_to_timespec(1, 1_000_000),
        Err(errno(EINVAL))
    );
    assert_eq!(ITimer::timeval_to_timespec(1, -1), Err(errno(EINVAL)));
    assert_eq!(ITimer::timeval_to_timespec(-1, 0), Err(errno(EINVAL)));
}

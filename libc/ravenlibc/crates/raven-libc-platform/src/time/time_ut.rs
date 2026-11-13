// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Time glue tests.

use raven_abi::errno::Errno;
use raven_libc_abi::structs::Timespec;
use raven_libc_abi::time;

use crate::backend::{FakeSys, Fault};
use crate::platform::Platform;
use crate::time::{clock_getres, clock_gettime, clock_nanosleep, gettimeofday, nanosleep};

#[test]
fn realtime_and_monotonic_are_available() {
    let platform = Platform::new(FakeSys::new());
    let realtime = clock_gettime(&platform, time::CLOCK_REALTIME).expect("realtime");
    let monotonic = clock_gettime(&platform, time::CLOCK_MONOTONIC).expect("monotonic");
    assert!(realtime.tv_sec >= 1_700_000_000);
    assert!(monotonic.tv_sec >= 1_700_000_000);
}

#[test]
fn unknown_clock_is_einval() {
    let platform = Platform::new(FakeSys::new());
    assert_eq!(
        clock_gettime(&platform, time::CLOCK_PROCESS_CPUTIME_ID),
        Err(Errno::EINVAL)
    );
}

#[test]
fn clock_getres_is_one_nanosecond() {
    let resolution = clock_getres(time::CLOCK_MONOTONIC).expect("res");
    assert_eq!(resolution.tv_sec, 0);
    assert_eq!(resolution.tv_nsec, 1);
    assert_eq!(clock_getres(99), Err(Errno::EINVAL));
}

#[test]
fn nanosleep_advances_the_clock() {
    let fake = FakeSys::new();
    let platform = Platform::new(fake);
    let before = clock_gettime(&platform, time::CLOCK_MONOTONIC).expect("clock");
    assert_eq!(nanosleep(&platform, 1_000_000), Ok(0));
    let after = clock_gettime(&platform, time::CLOCK_MONOTONIC).expect("clock");
    assert_ne!(before.tv_nsec, after.tv_nsec);
    assert_eq!(platform.backend().sleeps(), alloc::vec![1_000_000]);
}

#[test]
fn interrupted_nanosleep_returns_the_full_request_until_pg_17() {
    let fake = FakeSys::new();
    fake.fail_next(Fault::Nanosleep, Errno::EINTR);
    let platform = Platform::new(fake);
    assert_eq!(nanosleep(&platform, 5_000_000), Ok(5_000_000));
}

#[test]
fn absolute_clock_nanosleep_is_enosys() {
    let platform = Platform::new(FakeSys::new());
    assert_eq!(
        clock_nanosleep(
            &platform,
            time::CLOCK_MONOTONIC,
            time::TIMER_ABSTIME,
            &Timespec {
                tv_sec: 1,
                tv_nsec: 0
            },
            None
        ),
        Err(Errno::ENOSYS)
    );
}

#[test]
fn relative_clock_nanosleep_sleeps() {
    let platform = Platform::new(FakeSys::new());
    clock_nanosleep(
        &platform,
        time::CLOCK_MONOTONIC,
        0,
        &Timespec {
            tv_sec: 0,
            tv_nsec: 250_000,
        },
        None,
    )
    .expect("sleep");
    assert_eq!(platform.backend().sleeps(), alloc::vec![250_000]);
}

#[test]
fn invalid_timespec_is_einval() {
    let platform = Platform::new(FakeSys::new());
    assert_eq!(
        clock_nanosleep(
            &platform,
            time::CLOCK_MONOTONIC,
            0,
            &Timespec {
                tv_sec: 0,
                tv_nsec: 1_000_000_000
            },
            None
        ),
        Err(Errno::EINVAL)
    );
}

#[test]
fn gettimeofday_matches_realtime() {
    let platform = Platform::new(FakeSys::new());
    let (seconds, micros) = gettimeofday(&platform).expect("gettimeofday");
    assert!(seconds >= 1_700_000_000);
    assert!(micros < 1_000_000);
}

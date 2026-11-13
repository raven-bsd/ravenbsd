// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Clock tests.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;
use crate::sys::testing::FakeSys;
use raven_libc_abi::time::{
    CLOCK_BOOTTIME, CLOCK_MONOTONIC, CLOCK_MONOTONIC_COARSE, CLOCK_MONOTONIC_RAW,
    CLOCK_PROCESS_CPUTIME_ID, CLOCK_REALTIME, CLOCK_REALTIME_COARSE, CLOCK_THREAD_CPUTIME_ID,
};

#[test]
fn the_clock_ids_map_to_the_two_platform_clocks() {
    for id in [
        CLOCK_REALTIME,
        CLOCK_MONOTONIC,
        CLOCK_MONOTONIC_RAW,
        CLOCK_REALTIME_COARSE,
        CLOCK_MONOTONIC_COARSE,
        CLOCK_BOOTTIME,
    ] {
        assert!(clock_id(id).is_some(), "{id}");
    }
    assert!(clock_id(CLOCK_PROCESS_CPUTIME_ID).is_none());
    assert!(clock_id(CLOCK_THREAD_CPUTIME_ID).is_none());
    assert!(clock_id(-1).is_none());
    assert!(clock_id(1000).is_none());
}

#[test]
fn time_and_gettimeofday_read_the_realtime_clock() {
    let sys = FakeSys::new(Timespec {
        tv_sec: 1_700_000_000,
        tv_nsec: 123_456_789,
    });
    assert_eq!(time(&sys), Ok(1_700_000_000));
    assert_eq!(gettimeofday(&sys), Ok((1_700_000_000, 123_456)));
}

#[test]
fn clock_gettime_rejects_cpu_clocks() {
    let sys = FakeSys::epoch();
    assert_eq!(
        clock_gettime(&sys, CLOCK_PROCESS_CPUTIME_ID),
        Err(errno(EINVAL))
    );
    assert_eq!(
        clock_gettime(&sys, CLOCK_THREAD_CPUTIME_ID),
        Err(errno(EINVAL))
    );
    assert_eq!(clock_gettime(&sys, 99), Err(errno(EINVAL)));
}

#[test]
fn clock_getres_is_one_nanosecond_for_the_supported_ids() {
    for id in [
        CLOCK_REALTIME,
        CLOCK_MONOTONIC,
        CLOCK_PROCESS_CPUTIME_ID,
        CLOCK_THREAD_CPUTIME_ID,
        CLOCK_MONOTONIC_RAW,
        CLOCK_REALTIME_COARSE,
        CLOCK_MONOTONIC_COARSE,
        CLOCK_BOOTTIME,
    ] {
        assert_eq!(clock_getres(id).expect("res").tv_nsec, 1);
    }
    assert_eq!(clock_getres(99), Err(errno(EINVAL)));
}

#[test]
fn clock_settime_is_eperm_until_pg17() {
    let ts = Timespec::ZERO;
    assert_eq!(clock_settime(CLOCK_REALTIME, &ts), Err(errno(EPERM)));
    assert_eq!(clock_settime(99, &ts), Err(errno(EINVAL)));
}

#[test]
fn timespec_get_only_accepts_time_utc() {
    let sys = FakeSys::new(Timespec {
        tv_sec: 1,
        tv_nsec: 2,
    });
    assert_eq!(timespec_get(&sys, 1).expect("utc").tv_sec, 1);
    assert_eq!(timespec_get(&sys, 2), Err(()));
}

#[test]
fn cpu_time_functions_report_the_missing_primitive() {
    assert_eq!(clock(), -1);
    assert_eq!(times(), Err(errno(ENOSYS)));
    assert_eq!(adjtime(), errno(EPERM));
}

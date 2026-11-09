// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Time mapping tests.

mod common;

use common::MockHost;
use linux_shim::time::{clock_getres, clock_gettime, gettimeofday, time};
use raven_abi::errno::Errno;
use raven_abi::types::Timespec;
use raven_linux_abi::flags::clock;

fn host() -> MockHost {
    let mut host = MockHost::new();
    host.clock = (123, 456_789);
    host
}

#[test]
fn realtime_maps_to_the_realtime_native_clock() {
    let mut host = host();
    assert_eq!(
        clock_gettime(&mut host, clock::CLOCK_REALTIME).unwrap(),
        Timespec {
            tv_sec: 123,
            tv_nsec: 456_789
        }
    );
    assert_eq!(host.last_clock, Some(0));
}

#[test]
fn monotonic_maps_to_the_monotonic_native_clock() {
    let mut host = host();
    clock_gettime(&mut host, clock::CLOCK_MONOTONIC).unwrap();
    assert_eq!(host.last_clock, Some(1));
}

#[test]
fn monotonic_raw_maps_to_monotonic() {
    let mut host = host();
    clock_gettime(&mut host, clock::CLOCK_MONOTONIC_RAW).unwrap();
    assert_eq!(host.last_clock, Some(1));
}

#[test]
fn boottime_maps_to_monotonic() {
    let mut host = host();
    clock_gettime(&mut host, clock::CLOCK_BOOTTIME).unwrap();
    assert_eq!(host.last_clock, Some(1));
}

#[test]
fn monotonic_coarse_maps_to_monotonic() {
    let mut host = host();
    clock_gettime(&mut host, clock::CLOCK_MONOTONIC_COARSE).unwrap();
    assert_eq!(host.last_clock, Some(1));
}

#[test]
fn realtime_coarse_maps_to_realtime() {
    let mut host = host();
    clock_gettime(&mut host, clock::CLOCK_REALTIME_COARSE).unwrap();
    assert_eq!(host.last_clock, Some(0));
}

#[test]
fn unknown_clock_is_einval() {
    assert_eq!(
        clock_gettime(&mut host(), clock::CLOCK_PROCESS_CPUTIME_ID),
        Err(Errno::EINVAL)
    );
}

#[test]
fn gettimeofday_returns_microseconds() {
    assert_eq!(gettimeofday(&mut host()).unwrap(), (123, 456));
}

#[test]
fn time_returns_seconds() {
    assert_eq!(time(&mut host()).unwrap(), 123);
}

#[test]
fn normal_clock_resolution_is_one_nanosecond() {
    assert_eq!(
        clock_getres(clock::CLOCK_REALTIME).unwrap(),
        Timespec {
            tv_sec: 0,
            tv_nsec: 1
        }
    );
}

#[test]
fn coarse_clock_resolution_is_one_millisecond() {
    assert_eq!(
        clock_getres(clock::CLOCK_MONOTONIC_COARSE).unwrap(),
        Timespec {
            tv_sec: 0,
            tv_nsec: 1_000_000
        }
    );
}

#[test]
fn unknown_clock_resolution_is_einval() {
    assert_eq!(
        clock_getres(clock::CLOCK_THREAD_CPUTIME_ID),
        Err(Errno::EINVAL)
    );
}

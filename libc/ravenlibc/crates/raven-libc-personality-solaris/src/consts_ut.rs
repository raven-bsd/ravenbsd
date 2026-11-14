// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Tests for `consts.rs` (C67 4.3).

use crate::consts::*;

#[test]
fn port_source_values_are_the_documented_ones() {
    assert_eq!(PORT_SOURCE_TIMER, 1);
    assert_eq!(PORT_SOURCE_FD, 4);
}

#[test]
fn our_port_source_values_are_stable() {
    assert_eq!(PORT_SOURCE_USER, 3);
    assert_eq!(PORT_SOURCE_ALERT, 5);
}

#[test]
fn port_sources_table_matches_the_constants() {
    assert_eq!(
        PORT_SOURCES,
        &[
            ("PORT_SOURCE_TIMER", 1),
            ("PORT_SOURCE_USER", 3),
            ("PORT_SOURCE_FD", 4),
            ("PORT_SOURCE_ALERT", 5),
        ]
    );
}

#[test]
fn thr_flag_values() {
    assert_eq!(THR_BOUND, 0x1);
    assert_eq!(THR_NEW_LWP, 0x2);
    assert_eq!(THR_DETACHED, 0x4);
    assert_eq!(THR_SUSPENDED, 0x8);
    assert_eq!(THR_DAEMON, 0x10);
}

#[test]
fn thr_flags_table_matches_the_constants() {
    assert_eq!(THR_FLAGS.len(), 5);
    assert!(THR_FLAGS.contains(&("THR_DETACHED", 0x4)));
}

#[test]
fn usync_values() {
    assert_eq!(USYNC_THREAD, 0);
    assert_eq!(USYNC_PROCESS, 1);
    assert_eq!(USYNC_TYPES, &[("USYNC_THREAD", 0), ("USYNC_PROCESS", 1)]);
}

#[test]
fn stack_min_is_the_frozen_value() {
    assert_eq!(PTHREAD_STACK_MIN, 16_384);
}

#[test]
fn port_limits() {
    assert_eq!(PORT_MAX_ASSOCIATIONS, 4096);
    assert_eq!(PORT_MAX_QUEUED_USER_EVENTS, 1024);
    assert_eq!(PORT_MAX_PORTS, 256);
}

#[test]
fn port_event_is_twenty_four_bytes() {
    assert_eq!(core::mem::size_of::<PortEvent>(), 24);
    assert_eq!(core::mem::align_of::<PortEvent>(), 8);
}

#[test]
fn port_event_field_offsets() {
    assert_eq!(core::mem::offset_of!(PortEvent, portev_events), 0);
    assert_eq!(core::mem::offset_of!(PortEvent, portev_source), 4);
    assert_eq!(core::mem::offset_of!(PortEvent, portev_pad), 6);
    assert_eq!(core::mem::offset_of!(PortEvent, portev_object), 8);
    assert_eq!(core::mem::offset_of!(PortEvent, portev_user), 16);
}

#[test]
fn absent_timeout_waits_forever() {
    assert_eq!(timeout_ns(None), Ok(0));
}

#[test]
fn zero_timeout_polls() {
    assert_eq!(timeout_ns(Some(&Timespec::ZERO)), Ok(1));
}

#[test]
fn positive_timeout_is_nanoseconds() {
    let value = Timespec {
        tv_sec: 1,
        tv_nsec: 250_000_000,
    };
    assert_eq!(timeout_ns(Some(&value)), Ok(1_250_000_000));
}

#[test]
fn nanosecond_only_timeout() {
    let value = Timespec {
        tv_sec: 0,
        tv_nsec: 10_000_000,
    };
    assert_eq!(timeout_ns(Some(&value)), Ok(10_000_000));
}

#[test]
fn negative_seconds_are_invalid() {
    let value = Timespec {
        tv_sec: -1,
        tv_nsec: 0,
    };
    assert_eq!(timeout_ns(Some(&value)), Err(Errno::EINVAL));
}

#[test]
fn negative_nanoseconds_are_invalid() {
    let value = Timespec {
        tv_sec: 0,
        tv_nsec: -1,
    };
    assert_eq!(timeout_ns(Some(&value)), Err(Errno::EINVAL));
}

#[test]
fn nanoseconds_at_one_second_are_invalid() {
    let value = Timespec {
        tv_sec: 0,
        tv_nsec: 1_000_000_000,
    };
    assert_eq!(timeout_ns(Some(&value)), Err(Errno::EINVAL));
}

#[test]
fn huge_timeout_saturates() {
    let value = Timespec {
        tv_sec: i64::MAX,
        tv_nsec: 999_999_999,
    };
    assert_eq!(timeout_ns(Some(&value)), Ok(u64::MAX));
}

#[test]
fn etime_and_enotsup_are_linux_values() {
    assert_eq!(ETIME, Errno(62));
    assert_eq!(ENOTSUP, Errno(95));
}

#[test]
fn doorbell_cookie_cannot_name_a_descriptor() {
    assert_eq!(crate::port::cookie_fd(DOORBELL_COOKIE), -1);
}

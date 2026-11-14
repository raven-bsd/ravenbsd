// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `timerfd` clocks, flags and the `PG-19` gap.

use super::*;
use crate::seams::{CLOCK_BOOTTIME, CLOCK_MONOTONIC, CLOCK_REALTIME, Handle};
use crate::testutil::{FakeFdTable, FakeTimer, header_define};

const TIMER_H: &str = include_str!("../../../include/sys/timerfd.h");

fn setup(clock: i32, flags: i32) -> (FakeFdTable, FakeTimer, TimerFdTable, i32) {
    let mut table = FakeFdTable::new();
    let mut timer = FakeTimer::new();
    let mut state = TimerFdTable::new();
    let fd = create(&mut table, &mut timer, &mut state, clock, flags).unwrap();
    (table, timer, state, fd)
}

#[test]
fn header_flags_match() {
    assert_eq!(header_define(TIMER_H, "TFD_TIMER_ABSTIME"), Some(1));
    assert_eq!(header_define(TIMER_H, "TFD_TIMER_CANCEL_ON_SET"), Some(2));
}

#[test]
fn all_three_clocks_are_accepted() {
    for clock in [CLOCK_REALTIME, CLOCK_MONOTONIC, CLOCK_BOOTTIME] {
        assert!(clock_supported(clock));
        let (_, timer, _, _) = setup(clock, 0);
        assert_eq!(timer.created, alloc::vec![clock]);
    }
}

#[test]
fn unknown_clock_is_einval() {
    let (mut table, mut timer, mut state) =
        (FakeFdTable::new(), FakeTimer::new(), TimerFdTable::new());
    assert_eq!(
        create(&mut table, &mut timer, &mut state, 3, 0),
        Err(Errno::EINVAL)
    );
}

#[test]
fn unknown_flag_is_einval() {
    let (mut table, mut timer, mut state) =
        (FakeFdTable::new(), FakeTimer::new(), TimerFdTable::new());
    assert_eq!(
        create(&mut table, &mut timer, &mut state, 0, 0x80),
        Err(Errno::EINVAL)
    );
}

#[test]
fn create_stores_the_clock_and_cloexec() {
    let (table, _timer, state, fd) = setup(CLOCK_MONOTONIC, TFD_CLOEXEC);
    assert!(table.cloexec(fd));
    assert_eq!(
        state.get(fd).map(|timer| timer.clock),
        Some(CLOCK_MONOTONIC)
    );
}

#[test]
fn settime_relative_arms_the_timer() {
    let (table, mut timer, mut state, fd) = setup(CLOCK_MONOTONIC, 0);
    settime(&table, &mut timer, &mut state, fd, 0, 10, 5).unwrap();
    assert_eq!(timer.sets.last(), Some(&(Handle(301), 10, 5)));
    assert_eq!(state.get(fd).map(|entry| entry.initial_ns), Some(10));
}

#[test]
fn settime_rejects_unknown_flags() {
    let (table, mut timer, mut state, fd) = setup(CLOCK_MONOTONIC, 0);
    assert_eq!(
        settime(&table, &mut timer, &mut state, fd, 4, 0, 0),
        Err(Errno::EINVAL)
    );
}

#[test]
fn abstime_is_documented_einval() {
    let (table, mut timer, mut state, fd) = setup(CLOCK_MONOTONIC, 0);
    assert_eq!(
        settime(&table, &mut timer, &mut state, fd, TFD_TIMER_ABSTIME, 10, 0),
        Err(Errno::EINVAL)
    );
}

#[test]
fn cancel_on_set_requires_abstime() {
    let (table, mut timer, mut state, fd) = setup(CLOCK_MONOTONIC, 0);
    assert_eq!(
        settime(
            &table,
            &mut timer,
            &mut state,
            fd,
            TFD_TIMER_CANCEL_ON_SET,
            10,
            0
        ),
        Err(Errno::EINVAL)
    );
}

#[test]
fn settime_wrong_kind_is_einval() {
    let mut table = FakeFdTable::new();
    table.insert(10, Handle(710), FdKind::Pipe);
    let mut timer = FakeTimer::new();
    let mut state = TimerFdTable::new();
    assert_eq!(
        settime(&table, &mut timer, &mut state, 10, 0, 0, 0),
        Err(Errno::EINVAL)
    );
}

#[test]
fn gettime_needs_pg19() {
    let (table, mut timer, state, fd) = setup(CLOCK_MONOTONIC, 0);
    timer.unavailable = true;
    assert_eq!(gettime(&table, &mut timer, &state, fd), Err(Errno::ENOSYS));
}

#[test]
fn read_needs_pg19() {
    let (table, mut timer, _state, fd) = setup(CLOCK_MONOTONIC, 0);
    timer.unavailable = true;
    let mut buffer = [0u8; 8];
    assert_eq!(
        read(&table, &mut timer, fd, &mut buffer),
        Err(Errno::ENOSYS)
    );
}

#[test]
fn read_short_buffer_is_einval() {
    let (table, mut timer, _state, fd) = setup(CLOCK_MONOTONIC, 0);
    let mut buffer = [0u8; 4];
    assert_eq!(
        read(&table, &mut timer, fd, &mut buffer),
        Err(Errno::EINVAL)
    );
}

#[test]
fn disarm_stores_zero() {
    let (table, mut timer, mut state, fd) = setup(CLOCK_MONOTONIC, 0);
    settime(&table, &mut timer, &mut state, fd, 0, 10, 5).unwrap();
    settime(&table, &mut timer, &mut state, fd, 0, 0, 0).unwrap();
    assert_eq!(state.get(fd).map(|entry| entry.initial_ns), Some(0));
}

#[test]
fn create_install_error_propagates() {
    let mut table = FakeFdTable::new();
    table.install_error = Some(Errno::EMFILE);
    let mut timer = FakeTimer::new();
    let mut state = TimerFdTable::new();
    assert_eq!(
        create(&mut table, &mut timer, &mut state, 0, 0),
        Err(Errno::EMFILE)
    );
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `eventfd` counter semantics (ticket C66 5.H family `eventfd / timerfd`).

use super::*;
use crate::seams::Handle;
use crate::testutil::{FakeEvent, FakeFdTable};

fn setup(initval: u32, flags: i32) -> (FakeFdTable, FakeEvent, EventFdTable, i32) {
    let mut table = FakeFdTable::new();
    let mut event = FakeEvent::new();
    let mut state = EventFdTable::new();
    let fd = create(&mut table, &mut event, &mut state, initval, flags).unwrap();
    (table, event, state, fd)
}

#[test]
fn create_rejects_unknown_flags() {
    let (mut table, mut event, mut state) =
        (FakeFdTable::new(), FakeEvent::new(), EventFdTable::new());
    assert_eq!(
        create(&mut table, &mut event, &mut state, 0, 0x1000),
        Err(Errno::EINVAL)
    );
}

#[test]
fn create_stores_counter_and_cloexec() {
    let (table, _event, state, fd) = setup(7, EFD_CLOEXEC);
    assert!(table.cloexec(fd));
    assert_eq!(state.get(fd).map(|counter| counter.value), Some(7));
    assert_eq!(table.kind_of(fd), Ok(FdKind::EventFd));
}

#[test]
fn create_install_error_propagates() {
    let mut table = FakeFdTable::new();
    table.install_error = Some(Errno::EMFILE);
    let mut event = FakeEvent::new();
    let mut state = EventFdTable::new();
    assert_eq!(
        create(&mut table, &mut event, &mut state, 0, 0),
        Err(Errno::EMFILE)
    );
}

#[test]
fn write_adds_and_signals() {
    let (table, mut event, mut state, fd) = setup(1, 0);
    write(&table, &mut event, &mut state, fd, 5).unwrap();
    assert_eq!(state.get(fd).map(|counter| counter.value), Some(6));
    assert_eq!(event.signals.len(), 1);
}

#[test]
fn read_returns_the_whole_counter_and_clears() {
    let (table, mut event, mut state, fd) = setup(9, 0);
    let mut buffer = [0u8; 8];
    assert_eq!(read(&table, &mut event, &mut state, fd, &mut buffer), Ok(8));
    assert_eq!(u64::from_le_bytes(buffer), 9);
    assert_eq!(state.get(fd).map(|counter| counter.value), Some(0));
}

#[test]
fn semaphore_read_returns_one() {
    let (table, mut event, mut state, fd) = setup(3, EFD_SEMAPHORE);
    let mut buffer = [0u8; 8];
    assert_eq!(read(&table, &mut event, &mut state, fd, &mut buffer), Ok(8));
    assert_eq!(u64::from_le_bytes(buffer), 1);
    assert_eq!(state.get(fd).map(|counter| counter.value), Some(2));
}

#[test]
fn read_short_buffer_is_einval() {
    let (table, mut event, mut state, fd) = setup(1, 0);
    let mut buffer = [0u8; 4];
    assert_eq!(
        read(&table, &mut event, &mut state, fd, &mut buffer),
        Err(Errno::EINVAL)
    );
}

#[test]
fn read_wrong_kind_is_einval() {
    let mut table = FakeFdTable::new();
    table.insert(10, Handle(710), FdKind::Pipe);
    let mut event = FakeEvent::new();
    let mut state = EventFdTable::new();
    let mut buffer = [0u8; 8];
    assert_eq!(
        read(&table, &mut event, &mut state, 10, &mut buffer),
        Err(Errno::EINVAL)
    );
}

#[test]
fn read_closed_fd_is_ebadf() {
    let (table, mut event, mut state) = (FakeFdTable::new(), FakeEvent::new(), EventFdTable::new());
    let mut buffer = [0u8; 8];
    assert_eq!(
        read(&table, &mut event, &mut state, 44, &mut buffer),
        Err(Errno::EBADF)
    );
}

#[test]
fn read_empty_nonblock_is_eagain() {
    let (table, mut event, mut state, fd) = setup(0, EFD_NONBLOCK);
    let mut buffer = [0u8; 8];
    assert_eq!(
        read(&table, &mut event, &mut state, fd, &mut buffer),
        Err(Errno::EAGAIN)
    );
}

#[test]
fn read_empty_blocking_needs_pg19() {
    let (table, mut event, mut state, fd) = setup(0, 0);
    event.unavailable = true;
    let mut buffer = [0u8; 8];
    assert_eq!(
        read(&table, &mut event, &mut state, fd, &mut buffer),
        Err(Errno::ENOSYS)
    );
}

#[test]
fn write_maximum_value_is_einval() {
    let (table, mut event, mut state, fd) = setup(0, 0);
    assert_eq!(
        write(&table, &mut event, &mut state, fd, u64::MAX),
        Err(Errno::EINVAL)
    );
}

#[test]
fn write_overflow_is_eagain() {
    let (table, mut event, mut state, fd) = setup(u32::MAX, 0);
    let remaining = COUNTER_MAX.saturating_sub(u64::from(u32::MAX));
    write(&table, &mut event, &mut state, fd, remaining).unwrap();
    assert_eq!(
        write(&table, &mut event, &mut state, fd, 1),
        Err(Errno::EAGAIN)
    );
}

#[test]
fn write_reaches_the_maximum_exactly() {
    let (table, mut event, mut state, fd) = setup(0, 0);
    write(&table, &mut event, &mut state, fd, COUNTER_MAX).unwrap();
    assert_eq!(
        state.get(fd).map(|counter| counter.value),
        Some(COUNTER_MAX)
    );
}

#[test]
fn write_wrong_kind_is_einval() {
    let mut table = FakeFdTable::new();
    table.insert(10, Handle(710), FdKind::Pipe);
    let mut event = FakeEvent::new();
    let mut state = EventFdTable::new();
    assert_eq!(
        write(&table, &mut event, &mut state, 10, 1),
        Err(Errno::EINVAL)
    );
}

#[test]
fn two_creates_are_independent() {
    let (mut table, mut event, mut state, first) = setup(1, 0);
    let second = create(&mut table, &mut event, &mut state, 2, 0).unwrap();
    assert_ne!(first, second);
    write(&table, &mut event, &mut state, first, 10).unwrap();
    assert_eq!(state.get(first).map(|counter| counter.value), Some(11));
    assert_eq!(state.get(second).map(|counter| counter.value), Some(2));
}

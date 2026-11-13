// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! fd table tests.

use raven_abi::errno::Errno;

use crate::fd::table::{FdTable, FD_CLOEXEC};

fn table() -> FdTable {
    FdTable::new(64, 256)
}

#[test]
fn starts_empty() {
    let table = table();
    assert_eq!(table.open_count(), 0);
    assert!(!table.is_open(0));
    assert_eq!(table.soft_limit(), 64);
    assert_eq!(table.hard_limit(), 256);
}

#[test]
fn lowest_free_is_zero() {
    let mut table = table();
    assert_eq!(table.alloc(0), Ok(0));
    assert_eq!(table.alloc(0), Ok(1));
    assert_eq!(table.alloc(0), Ok(2));
}

#[test]
fn stdio_numbers_can_be_installed() {
    let mut table = table();
    for fd in 0..3 {
        table.install_at(fd, fd + 1, false, 0).expect("install");
    }
    assert!(table.is_open(0));
    assert!(table.is_open(1));
    assert!(table.is_open(2));
    assert_eq!(table.open_count(), 3);
}

#[test]
fn min_fd_is_honoured() {
    let mut table = table();
    assert_eq!(table.alloc(10), Ok(10));
    assert_eq!(table.alloc(5), Ok(5));
    assert_eq!(table.alloc(5), Ok(6));
}

#[test]
fn min_above_soft_limit_is_einval() {
    let mut table = table();
    assert_eq!(table.alloc(64), Err(Errno::EINVAL));
}

#[test]
fn full_table_is_emfile() {
    let mut table = FdTable::new(4, 8);
    for fd in 0..4 {
        assert_eq!(table.alloc(0), Ok(fd));
    }
    assert_eq!(table.alloc(0), Err(Errno::EMFILE));
}

#[test]
fn remove_frees_the_slot() {
    let mut table = table();
    let fd = table.alloc(0).expect("alloc");
    assert_eq!(table.alloc(0), Ok(1));
    let entry = table.remove(fd as i32).expect("removed");
    assert_eq!(entry.ofd, 0);
    assert_eq!(table.alloc(0), Ok(0));
}

#[test]
fn remove_unknown_is_none() {
    let mut table = table();
    assert!(table.remove(3).is_none());
    assert!(table.remove(-1).is_none());
}

#[test]
fn growth_beyond_inline_entries() {
    let mut table = FdTable::new(300, 300);
    for _ in 0..130 {
        table.alloc(0).expect("alloc");
    }
    assert_eq!(table.open_count(), 130);
    assert!(table.is_open(129));
    assert_eq!(table.alloc(0), Ok(130));
}

#[test]
fn outside_hard_limit_is_ebadf() {
    let mut table = FdTable::new(8, 8);
    assert_eq!(table.install_at(8, 1, false, 0), Err(Errno::EBADF));
    assert!(table.entry(8).is_none());
}

#[test]
fn close_from_removes_upper_descriptors() {
    let mut table = table();
    for _ in 0..6 {
        table.alloc(0).expect("alloc");
    }
    let removed = table.close_from(3);
    assert_eq!(removed.len(), 3);
    assert_eq!(table.open_count(), 3);
    assert!(table.is_open(2));
    assert!(!table.is_open(3));
}

#[test]
fn close_range_is_half_open() {
    let mut table = table();
    for _ in 0..6 {
        table.alloc(0).expect("alloc");
    }
    let removed = table.close_range(2, 4);
    assert_eq!(removed.len(), 2);
    assert!(table.is_open(1));
    assert!(!table.is_open(2));
    assert!(!table.is_open(3));
    assert!(table.is_open(4));
}

#[test]
fn cloexec_round_trip() {
    let mut table = table();
    let fd = table.alloc(0).expect("alloc");
    table.set_cloexec(fd as i32, true).expect("set");
    assert!(table.entry(fd as i32).expect("entry").cloexec());
    table.set_cloexec(fd as i32, false).expect("clear");
    assert!(!table.entry(fd as i32).expect("entry").cloexec());
}

#[test]
fn install_at_records_flags_and_tag() {
    let mut table = table();
    table.install_at(4, 7, true, 0x1234).expect("install");
    let entry = table.entry(4).expect("entry");
    assert_eq!(entry.ofd, 7);
    assert_eq!(entry.fd_flags, FD_CLOEXEC);
    assert_eq!(entry.tag, 0x1234);
}

#[test]
fn tag_read_set() {
    let mut table = table();
    table.install_at(0, 1, false, 0).expect("install");
    assert_eq!(table.tag(0), 0);
    assert_eq!(table.set_tag(0, 99), Ok(0));
    assert_eq!(table.tag(0), 99);
    assert_eq!(table.set_tag(1, 1), Err(Errno::EBADF));
}

#[test]
fn open_entries_is_ordered() {
    let mut table = table();
    table.install_at(5, 2, false, 0).expect("install");
    table.install_at(1, 1, false, 0).expect("install");
    let entries = table.open_entries();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].0, 1);
    assert_eq!(entries[1].0, 5);
}

#[test]
fn limits_can_grow_and_shrink() {
    let mut table = FdTable::new(8, 16);
    table.set_hard_limit(32);
    assert_eq!(table.hard_limit(), 32);
    table.set_soft_limit(4);
    assert_eq!(table.soft_limit(), 4);
    table.set_soft_limit(64);
    assert_eq!(table.soft_limit(), 32);
}

#[test]
fn negative_fd_never_exists() {
    let table = table();
    assert!(table.entry(-1).is_none());
    assert!(table.entry(i32::MIN).is_none());
}

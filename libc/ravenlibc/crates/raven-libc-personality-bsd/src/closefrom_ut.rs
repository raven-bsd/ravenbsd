// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `closefrom`/`close_range` table (ticket C66 5.H family `closefrom / close_range`).

use super::*;
use crate::seams::{FdKind, Handle};
use crate::testutil::FakeFdTable;

fn table_with(fds: &[i32]) -> FakeFdTable {
    let mut table = FakeFdTable::new();
    for fd in fds {
        table.insert(*fd, Handle(*fd as u32), FdKind::File);
    }
    table
}

#[test]
fn closefrom_zero_closes_everything() {
    let mut table = table_with(&[0, 1, 2, 3, 9]);
    closefrom(&mut table, 0);
    assert!(table.is_empty());
}

#[test]
fn closefrom_negative_is_zero() {
    let mut table = table_with(&[0, 1, 2]);
    closefrom(&mut table, -5);
    assert!(table.is_empty());
}

#[test]
fn closefrom_threshold() {
    let mut table = table_with(&[1, 2, 3, 10, 11]);
    closefrom(&mut table, 3);
    assert!(table.is_open(1));
    assert!(table.is_open(2));
    assert!(!table.is_open(3));
    assert!(!table.is_open(11));
}

#[test]
fn closefrom_empty_table_is_noop() {
    let mut table = FakeFdTable::new();
    closefrom(&mut table, 3);
    assert!(table.is_empty());
}

#[test]
fn close_range_closes_inclusive() {
    let mut table = table_with(&[1, 2, 3, 4, 5]);
    assert_eq!(close_range(&mut table, 2, 4, 0), Ok(()));
    assert!(table.is_open(1));
    assert!(!table.is_open(2));
    assert!(!table.is_open(4));
    assert!(table.is_open(5));
}

#[test]
fn close_range_reversed_is_einval() {
    let mut table = table_with(&[3]);
    assert_eq!(close_range(&mut table, 4, 2, 0), Err(Errno::EINVAL));
    assert!(table.is_open(3));
}

#[test]
fn close_range_unshare_is_einval() {
    let mut table = table_with(&[3]);
    assert_eq!(
        close_range(&mut table, 0, 9, CLOSE_RANGE_UNSHARE),
        Err(Errno::EINVAL)
    );
}

#[test]
fn close_range_unknown_flag_is_einval() {
    let mut table = table_with(&[3]);
    assert_eq!(close_range(&mut table, 0, 9, 0x100), Err(Errno::EINVAL));
}

#[test]
fn close_range_cloexec_marks_instead_of_closing() {
    let mut table = table_with(&[2, 3, 4]);
    assert_eq!(close_range(&mut table, 2, 3, CLOSE_RANGE_CLOEXEC), Ok(()));
    assert!(table.is_open(2));
    assert!(table.is_open(3));
    assert!(table.cloexec(2));
    assert!(table.cloexec(3));
    assert!(!table.cloexec(4));
}

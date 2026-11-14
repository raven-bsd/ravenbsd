// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `inotify` flag validation and the `PG-38` gap.

use super::*;
use crate::seams::Handle;
use crate::testutil::{FakeFdTable, FakeWatch, header_define};

const INOTIFY_H: &str = include_str!("../../../include/sys/inotify.h");

#[test]
fn header_event_bits_match() {
    for (name, value) in [
        ("IN_ACCESS", IN_ACCESS),
        ("IN_MODIFY", IN_MODIFY),
        ("IN_ATTRIB", IN_ATTRIB),
        ("IN_CLOSE_WRITE", IN_CLOSE_WRITE),
        ("IN_CLOSE_NOWRITE", IN_CLOSE_NOWRITE),
        ("IN_OPEN", IN_OPEN),
        ("IN_MOVED_FROM", IN_MOVED_FROM),
        ("IN_MOVED_TO", IN_MOVED_TO),
        ("IN_CREATE", IN_CREATE),
        ("IN_DELETE", IN_DELETE),
        ("IN_DELETE_SELF", IN_DELETE_SELF),
        ("IN_MOVE_SELF", IN_MOVE_SELF),
        ("IN_ALL_EVENTS", IN_ALL_EVENTS),
        ("IN_ONLYDIR", IN_ONLYDIR),
        ("IN_DONT_FOLLOW", IN_DONT_FOLLOW),
        ("IN_EXCL_UNLINK", IN_EXCL_UNLINK),
        ("IN_MASK_CREATE", IN_MASK_CREATE),
        ("IN_MASK_ADD", IN_MASK_ADD),
        ("IN_ONESHOT", IN_ONESHOT),
    ] {
        assert_eq!(
            header_define(INOTIFY_H, name),
            Some(i64::from(value)),
            "{name}"
        );
    }
}

#[test]
fn mask_valid_accepts_an_event_bit() {
    assert!(mask_valid(IN_ACCESS));
    assert!(mask_valid(IN_ALL_EVENTS));
    assert!(mask_valid(IN_CREATE | IN_ONESHOT));
}

#[test]
fn mask_without_event_bits_is_invalid() {
    assert!(!mask_valid(0));
    assert!(!mask_valid(IN_ONLYDIR));
    assert!(!mask_valid(IN_MASK_ADD));
}

#[test]
fn mask_with_unknown_bits_is_invalid() {
    assert!(!mask_valid(IN_ACCESS | 0x4000_0000u32));
    assert!(!mask_valid(IN_ACCESS | 0x4000));
}

#[test]
fn init1_bad_flag_is_einval() {
    let mut table = FakeFdTable::new();
    let mut watch = FakeWatch::new();
    assert_eq!(init1(&mut table, &mut watch, 0x100), Err(Errno::EINVAL));
}

#[test]
fn init_needs_pg38() {
    let mut table = FakeFdTable::new();
    let mut watch = FakeWatch::new();
    assert_eq!(init(&mut table, &mut watch), Err(Errno::ENOSYS));
}

#[test]
fn init1_installs_when_available() {
    let mut table = FakeFdTable::new();
    let mut watch = FakeWatch::new();
    watch.available = true;
    let fd = init1(&mut table, &mut watch, IN_CLOEXEC).unwrap();
    assert_eq!(table.kind_of(fd), Ok(FdKind::Inotify));
    assert!(table.cloexec(fd));
}

#[test]
fn add_watch_validates_mask_before_service() {
    let mut table = FakeFdTable::new();
    let mut watch = FakeWatch::new();
    assert_eq!(
        add_watch(&mut table, &mut watch, 3, b"/tmp", 0),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        add_watch(&mut table, &mut watch, 3, b"/tmp", IN_ACCESS | 0x4000),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        add_watch(&mut table, &mut watch, 3, b"", IN_ACCESS),
        Err(Errno::EINVAL)
    );
}

#[test]
fn add_watch_needs_pg38() {
    let mut table = FakeFdTable::new();
    table.insert(3, Handle(703), FdKind::Inotify);
    let mut watch = FakeWatch::new();
    assert_eq!(
        add_watch(&mut table, &mut watch, 3, b"/tmp", IN_CREATE),
        Err(Errno::ENOSYS)
    );
}

#[test]
fn add_watch_wrong_fd_kind_is_einval() {
    let mut table = FakeFdTable::new();
    table.insert(3, Handle(703), FdKind::Pipe);
    let mut watch = FakeWatch::new();
    watch.available = true;
    assert_eq!(
        add_watch(&mut table, &mut watch, 3, b"/tmp", IN_CREATE),
        Err(Errno::EINVAL)
    );
}

#[test]
fn rm_watch_needs_pg38_and_valid_fd() {
    let mut table = FakeFdTable::new();
    table.insert(3, Handle(703), FdKind::Inotify);
    let mut watch = FakeWatch::new();
    assert_eq!(rm_watch(&mut table, &mut watch, 3, 1), Err(Errno::ENOSYS));
    assert_eq!(rm_watch(&mut table, &mut watch, 4, 1), Err(Errno::EBADF));
    assert_eq!(rm_watch(&mut table, &mut watch, 3, -1), Err(Errno::EINVAL));
}

#[test]
fn rm_watch_succeeds_when_available() {
    let mut table = FakeFdTable::new();
    table.insert(3, Handle(703), FdKind::Inotify);
    let mut watch = FakeWatch::new();
    watch.available = true;
    assert_eq!(rm_watch(&mut table, &mut watch, 3, 1), Ok(()));
}

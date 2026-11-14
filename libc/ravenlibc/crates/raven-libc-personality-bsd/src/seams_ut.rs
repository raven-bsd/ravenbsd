// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Seam helpers and the fd table contract.

use super::*;
use crate::testutil::FakeFdTable;

#[test]
fn pollable_kinds() {
    assert!(!FdKind::File.pollable());
    assert!(!FdKind::Directory.pollable());
    assert!(FdKind::Pipe.pollable());
    assert!(FdKind::Kqueue.pollable());
}

#[test]
fn fake_close_range_from_closes_at_or_above() {
    let mut table = FakeFdTable::new();
    for fd in [1, 2, 5, 9] {
        table.insert(fd, Handle(fd as u32), FdKind::File);
    }
    assert_eq!(table.close_range_from(5), 2);
    assert!(table.is_open(1));
    assert!(!table.is_open(9));
}

#[test]
fn fake_close_range_between_closes_inclusive() {
    let mut table = FakeFdTable::new();
    for fd in [1, 2, 3, 4, 5] {
        table.insert(fd, Handle(fd as u32), FdKind::File);
    }
    assert_eq!(table.close_range_between(2, 4), 3);
    assert!(table.is_open(1));
    assert!(table.is_open(5));
}

#[test]
fn default_mount_list_is_enosys() {
    #[derive(Default)]
    struct Bare;
    impl MountList for Bare {}
    let mut bare = Bare;
    assert_eq!(bare.list(), Err(Errno::ENOSYS));
}

#[test]
fn default_process_handle_is_enosys() {
    struct Bare;
    impl SrcOps for Bare {
        fn timer_create(&mut self, _clock: i32) -> Result<Handle, Errno> {
            Err(Errno::ENOSYS)
        }
        fn timer_set(
            &mut self,
            _handle: Handle,
            _initial: u64,
            _interval: u64,
        ) -> Result<(), Errno> {
            Err(Errno::ENOSYS)
        }
        fn event_create(&mut self) -> Result<Handle, Errno> {
            Err(Errno::ENOSYS)
        }
        fn event_signal(&mut self, _handle: Handle, _value: u64) -> Result<(), Errno> {
            Err(Errno::ENOSYS)
        }
    }
    let mut bare = Bare;
    assert_eq!(bare.process_handle(1), Err(Errno::ENOSYS));
}

#[test]
fn evq_ctl_values() {
    assert_eq!(EvqCtl::Add as u32, 1);
    assert_eq!(EvqCtl::Mod as u32, 2);
    assert_eq!(EvqCtl::Del as u32, 3);
}

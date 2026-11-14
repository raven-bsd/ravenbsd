// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Seam helpers and the fd table contract.

use super::*;
use crate::testutil::FakeFdTable;

#[test]
fn sigset_contains_each_signal() {
    let mut set = [0u64; 2];
    set[0] = 1u64 << 0;
    assert!(sigset_contains(&set, 1));
    assert!(!sigset_contains(&set, 2));
    set[1] = 1u64 << 0;
    assert!(!sigset_contains(&set, 65));
}

#[test]
fn sigset_rejects_out_of_range() {
    let set = [u64::MAX; 2];
    assert!(!sigset_contains(&set, 0));
    assert!(!sigset_contains(&set, 65));
}

#[test]
fn pollable_kinds() {
    assert!(!FdKind::File.pollable());
    assert!(!FdKind::Directory.pollable());
    assert!(FdKind::Pipe.pollable());
    assert!(FdKind::Tty.pollable());
    assert!(FdKind::Socket.pollable());
    assert!(FdKind::EventFd.pollable());
}

#[test]
fn fake_close_range_from_closes_only_at_or_above() {
    let mut table = FakeFdTable::new();
    for fd in [1, 2, 5, 9] {
        table.insert(fd, Handle(fd as u32), FdKind::File);
    }
    assert_eq!(table.close_range_from(5), 2);
    assert!(table.is_open(1));
    assert!(table.is_open(2));
    assert!(!table.is_open(5));
    assert!(!table.is_open(9));
}

#[test]
fn fake_close_range_between_uses_close_fd() {
    let mut table = FakeFdTable::new();
    for fd in [1, 2, 3, 4, 5] {
        table.insert(fd, Handle(fd as u32), FdKind::File);
    }
    assert_eq!(table.close_range_between(2, 4), 3);
    assert!(table.is_open(1));
    assert!(table.is_open(5));
}

#[test]
fn fake_close_range_between_reversed_is_empty() {
    let mut table = FakeFdTable::new();
    table.insert(3, Handle(3), FdKind::File);
    assert_eq!(table.close_range_between(4, 1), 0);
    assert!(table.is_open(3));
}

#[test]
fn fake_set_cloexec_reports_ebadf() {
    let mut table = FakeFdTable::new();
    assert_eq!(table.set_cloexec(1, true), Err(Errno::EBADF));
}

#[test]
fn default_seams_are_enosys() {
    struct Bare;
    impl EventOps for Bare {
        fn create(&mut self) -> Result<Handle, Errno> {
            Ok(Handle(1))
        }
        fn signal(&mut self, _handle: Handle) -> Result<(), Errno> {
            Ok(())
        }
    }
    let mut bare = Bare;
    assert_eq!(bare.read_counter(Handle(1)), Err(Errno::ENOSYS));
    assert_eq!(bare.wait_readable(Handle(1)), Err(Errno::ENOSYS));
}

#[test]
fn evq_ctl_values_are_the_abi_values() {
    assert_eq!(EvqCtl::Add as u32, 1);
    assert_eq!(EvqCtl::Mod as u32, 2);
    assert_eq!(EvqCtl::Del as u32, 3);
}

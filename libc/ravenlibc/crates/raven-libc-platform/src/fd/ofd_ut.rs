// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! OFD arena tests.

use raven_abi::errno::Errno;
use raven_abi::types::Handle;

use crate::fd::ofd::{Ofd, OfdArena, OfdKind};

#[test]
fn zero_is_reserved() {
    let arena = OfdArena::new();
    assert!(arena.get(0).is_none());
    assert_eq!(arena.refs(0), 0);
}

#[test]
fn insert_and_get() {
    let mut arena = OfdArena::new();
    let index = arena.insert(Handle(7), OfdKind::File, 0, None);
    assert_eq!(index, 1);
    let ofd = arena.get(index).expect("ofd");
    assert_eq!(ofd.handle.get(), Handle(7));
    assert_eq!(ofd.kind, OfdKind::File);
    assert_eq!(ofd.refs.load(core::sync::atomic::Ordering::Relaxed), 1);
}

#[test]
fn add_and_drop_refs() {
    let mut arena = OfdArena::new();
    let index = arena.insert(Handle(1), OfdKind::File, 0, None);
    assert_eq!(arena.add_ref(index), Ok(2));
    assert!(!arena.drop_ref(index));
    assert!(arena.drop_ref(index));
    assert_eq!(arena.refs(index), 0);
}

#[test]
fn adding_ref_to_a_dead_ofd_fails() {
    let mut arena = OfdArena::new();
    let index = arena.insert(Handle(1), OfdKind::File, 0, None);
    assert!(arena.drop_ref(index));
    assert_eq!(arena.add_ref(index), Err(Errno::EBADF));
}

#[test]
fn release_reuses_the_slot() {
    let mut arena = OfdArena::new();
    let first = arena.insert(Handle(1), OfdKind::File, 0, None);
    let _ = arena.drop_ref(first);
    let released = arena.release(first).expect("released");
    assert_eq!(released.handle.get(), Handle(1));
    let second = arena.insert(Handle(2), OfdKind::File, 0, None);
    assert_eq!(second, first, "the freed slot is reused");
}

#[test]
fn access_mode_helpers() {
    let ofd = Ofd::new(Handle(1), OfdKind::File, 0, None, 1);
    assert!(ofd.readable());
    assert!(!ofd.writable());
    let ofd = Ofd::new(Handle(1), OfdKind::File, 1, None, 1);
    assert!(!ofd.readable());
    assert!(ofd.writable());
    let ofd = Ofd::new(Handle(1), OfdKind::File, 2, None, 1);
    assert!(ofd.readable());
    assert!(ofd.writable());
}

#[test]
fn pending_ofd_attaches_once() {
    let mut ofd = Ofd::new(Handle::NONE, OfdKind::Socket, 0, None, 1);
    assert_eq!(ofd.handle.get(), Handle::NONE);
    assert_eq!(ofd.set_handle(Handle(9)), Ok(()));
    assert_eq!(ofd.handle.get(), Handle(9));
    assert_eq!(ofd.set_handle(Handle(10)), Err(Errno::EEXIST));
    let mut ofd = Ofd::new(Handle::NONE, OfdKind::Socket, 0, None, 2);
    assert_eq!(ofd.set_handle(Handle::NONE), Err(Errno::EINVAL));
}

#[test]
fn an_event_handle_becomes_the_poll_source() {
    let mut ofd = Ofd::new(Handle(4), OfdKind::Socket, 0, None, 1);
    assert_eq!(ofd.poll_handle(), Handle(4));
    assert!(!ofd.poll_signaled());
    assert_eq!(ofd.set_event(Handle(5)), Ok(()));
    assert_eq!(ofd.poll_handle(), Handle(5));
    assert!(ofd.poll_signaled());
    assert_eq!(ofd.set_event(Handle(6)), Err(Errno::EEXIST));
    let mut plain = Ofd::new(Handle(7), OfdKind::File, 0, None, 2);
    assert_eq!(plain.set_event(Handle::NONE), Err(Errno::EINVAL));
}

#[test]
fn epochs_increase() {
    let mut arena = OfdArena::new();
    let first = arena.insert(Handle(1), OfdKind::File, 0, None);
    let second = arena.insert(Handle(2), OfdKind::File, 0, None);
    let first_epoch = arena.get(first).expect("first").epoch;
    let second_epoch = arena.get(second).expect("second").epoch;
    assert_ne!(first_epoch, second_epoch);
}

#[test]
fn totals_and_entries() {
    let mut arena = OfdArena::new();
    let a = arena.insert(Handle(1), OfdKind::File, 0, None);
    let b = arena.insert(Handle(2), OfdKind::Dir, 0, None);
    assert_eq!(arena.live(), 2);
    assert_eq!(arena.total_refs(), 2);
    assert_eq!(arena.add_ref(a), Ok(2));
    assert_eq!(arena.total_refs(), 3);
    let entries = arena.entries();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].0, a);
    assert_eq!(entries[1].0, b);
}

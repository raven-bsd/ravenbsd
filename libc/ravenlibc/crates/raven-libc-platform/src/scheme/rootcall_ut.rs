// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Cached root-handle tests.

use raven_abi::errno::Errno;

use crate::backend::{FakeSys, Fault};
use crate::scheme::rootcall::RootCall;

#[test]
fn root_is_opened_once() {
    let fake = FakeSys::new();
    fake.dir(b"/");
    let root = RootCall::new();
    let first = root.ensure(&fake).expect("first");
    let second = root.ensure(&fake).expect("second");
    assert_eq!(first, second);
    assert_eq!(fake.open_handles(), 1);
}

#[test]
fn a_call_succeeds() {
    let fake = FakeSys::new();
    fake.dir(b"/");
    let root = RootCall::new();
    root.call(&fake, raven_proto_fs::Call::Sync).expect("sync");
}

#[test]
fn eio_reopens_the_root_once() {
    let fake = FakeSys::new();
    fake.dir(b"/");
    let root = RootCall::new();
    fake.fail_next(Fault::Call, Errno::EIO);
    root.call(&fake, raven_proto_fs::Call::Sync)
        .expect("reopen");
}

#[test]
fn invalidate_closes_the_handle() {
    let fake = FakeSys::new();
    fake.dir(b"/");
    let root = RootCall::new();
    let _ = root.ensure(&fake).expect("open");
    assert_eq!(fake.open_handles(), 1);
    root.invalidate(&fake);
    assert_eq!(fake.open_handles(), 0);
}

#[test]
fn open_error_propagates() {
    let fake = FakeSys::new();
    let root = RootCall::new();
    assert_eq!(root.ensure(&fake), Err(Errno::ENOENT));
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `pledge`/`unveil` glue table (ticket C66 5.H family `pledge glue`).

use super::*;
use crate::testutil::FakePledge;
use raven_libc_pledge::reduce::PledgeState;
use raven_libc_pledge::unveil::UnveilTable;

#[test]
fn null_null_pledge_is_a_noop() {
    let mut backend = FakePledge::new();
    let mut state = PledgeState::new();
    assert_eq!(pledge(&mut backend, &mut state, None, None), Ok(()));
    assert!(backend.applied.is_empty());
}

#[test]
fn every_promise_is_accepted() {
    let mut backend = FakePledge::new();
    let mut state = PledgeState::new();
    let all = b"stdio rpath wpath cpath dpath inet mcast fattr chown flock unix dns getpw sendfd recvfd tape tty proc exec prot_exec settime ps vminfo id pf route wroute audio video bpf unveil error disklabel drm vmm";
    assert_eq!(pledge(&mut backend, &mut state, Some(all), None), Ok(()));
    assert_eq!(backend.applied.len(), 1);
}

#[test]
fn unknown_promise_is_einval() {
    let mut backend = FakePledge::new();
    let mut state = PledgeState::new();
    assert_eq!(
        pledge(&mut backend, &mut state, Some(b"stdio nosuch"), None),
        Err(Errno(22))
    );
    assert!(backend.applied.is_empty());
}

#[test]
fn increasing_promises_is_eperm() {
    let mut backend = FakePledge::new();
    let mut state = PledgeState::new();
    pledge(&mut backend, &mut state, Some(b"stdio"), None).unwrap();
    assert_eq!(
        pledge(&mut backend, &mut state, Some(b"stdio rpath"), None),
        Err(Errno(1))
    );
}

#[test]
fn reducing_promises_is_allowed() {
    let mut backend = FakePledge::new();
    let mut state = PledgeState::new();
    pledge(&mut backend, &mut state, Some(b"stdio rpath"), None).unwrap();
    assert_eq!(
        pledge(&mut backend, &mut state, Some(b"stdio"), None),
        Ok(())
    );
}

#[test]
fn execpromises_are_stored() {
    let mut backend = FakePledge::new();
    let mut state = PledgeState::new();
    promise_with_exec(&mut backend, &mut state);
}

fn promise_with_exec(backend: &mut FakePledge, state: &mut PledgeState) {
    assert_eq!(pledge(backend, state, None, Some(b"stdio")), Ok(()));
    assert_eq!(backend.applied.len(), 1);
}

#[test]
fn unveil_single_entry() {
    let mut backend = FakePledge::new();
    let mut state = UnveilTable::new();
    assert_eq!(
        unveil(&mut backend, &mut state, Some(b"/etc"), Some(b"r")),
        Ok(())
    );
    assert_eq!(state.len(), 1);
    assert_eq!(backend.unveiled.len(), 1);
}

#[test]
fn unveil_null_null_locks() {
    let mut backend = FakePledge::new();
    let mut state = UnveilTable::new();
    assert_eq!(
        unveil(&mut backend, &mut state, Some(b"/"), Some(b"rwxc")),
        Ok(())
    );
    assert_eq!(unveil(&mut backend, &mut state, None, None), Ok(()));
    assert_eq!(
        unveil(&mut backend, &mut state, Some(b"/tmp"), Some(b"rw")),
        Err(Errno(1))
    );
}

#[test]
fn unveil_relative_path_is_einval() {
    let mut backend = FakePledge::new();
    let mut state = UnveilTable::new();
    assert_eq!(
        unveil(&mut backend, &mut state, Some(b"etc"), Some(b"r")),
        Err(Errno(22))
    );
}

#[test]
fn unveil_bad_permission_is_einval() {
    let mut backend = FakePledge::new();
    let mut state = UnveilTable::new();
    assert_eq!(
        unveil(&mut backend, &mut state, Some(b"/etc"), Some(b"z")),
        Err(Errno(22))
    );
}

#[test]
fn unveil_128_entries_allowed_129_is_e2big() {
    let mut backend = FakePledge::new();
    let mut state = UnveilTable::new();
    for index in 0..128u32 {
        let path = alloc::format!("/p{index}");
        assert_eq!(
            unveil(&mut backend, &mut state, Some(path.as_bytes()), Some(b"r")),
            Ok(())
        );
    }
    assert_eq!(
        unveil(&mut backend, &mut state, Some(b"/one-too-many"), Some(b"r")),
        Err(Errno(7))
    );
}

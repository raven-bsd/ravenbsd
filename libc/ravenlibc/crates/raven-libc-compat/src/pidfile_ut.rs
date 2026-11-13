// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `pidfile` tests over `FakePid`, adapted from `netbsd:tests/lib/libutil/t_pidfile.c`.

#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::byte_char_slices,
    clippy::field_reassign_with_default,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

use super::*;
use crate::testutil::FakePid;

fn hook() {}

#[test]
fn path_is_var_run_with_pid_suffix() {
    let mut ops = FakePid::default();
    let mut state = PidState::new(hook);

    pidfile(&mut ops, Some(b"mydaemon"), b"prog", &mut state).unwrap();
    assert_eq!(state.path, b"/var/run/mydaemon.pid");
    assert_eq!(ops.writes.len(), 1);
    assert_eq!(ops.writes[0].0, b"/var/run/mydaemon.pid");
    assert_eq!(ops.writes[0].1, b"4242\n");
}

#[test]
fn basename_defaults_to_the_program_name() {
    let mut ops = FakePid::default();
    let mut state = PidState::new(hook);
    pidfile(&mut ops, None, b"login", &mut state).unwrap();
    assert_eq!(state.path, b"/var/run/login.pid");
}

#[test]
fn empty_program_name_uses_the_donor_default() {
    let mut ops = FakePid::default();
    let mut state = PidState::new(hook);
    pidfile(&mut ops, None, b"", &mut state).unwrap();
    assert_eq!(state.path, b"/var/run/unknown.pid");
}

#[test]
fn atexit_is_registered_once() {
    let mut ops = FakePid::default();
    let mut state = PidState::new(hook);
    pidfile(&mut ops, Some(b"a"), b"p", &mut state).unwrap();
    assert_eq!(ops.atexits.len(), 1);
}

#[test]
fn atexit_failure_is_returned() {
    let mut ops = FakePid::default();
    ops.atexit_error = Some(Errno(raven_libc_abi::errno::ENOMEM as u16));
    let mut state = PidState::new(hook);
    let error = pidfile(&mut ops, Some(b"a"), b"p", &mut state).unwrap_err();
    assert_eq!(error, Errno(raven_libc_abi::errno::ENOMEM as u16));
    assert!(ops.writes.is_empty());
}

#[test]
fn write_failure_unlinks_and_keeps_the_error() {
    let mut ops = FakePid::default();
    ops.write_error = Some(Errno(raven_libc_abi::errno::EACCES as u16));
    let mut state = PidState::new(hook);
    let error = pidfile(&mut ops, Some(b"a"), b"p", &mut state).unwrap_err();
    assert_eq!(error, Errno(raven_libc_abi::errno::EACCES as u16));
    assert_eq!(ops.unlinks, alloc::vec![b"/var/run/a.pid".to_vec()]);
    assert!(!state.active());
}

#[test]
fn cleanup_unlinks_only_in_the_creating_pid() {
    let mut ops = FakePid::default();
    let mut state = PidState::new(hook);
    pidfile(&mut ops, Some(b"a"), b"p", &mut state).unwrap();
    cleanup(&mut ops, &state);
    assert_eq!(ops.unlinks.len(), 1);
    ops.pid = 1;
    cleanup(&mut ops, &state);
    assert_eq!(ops.unlinks.len(), 1);
}

#[test]
fn cleanup_without_a_pidfile_does_nothing() {
    let mut ops = FakePid::default();
    let state = PidState::new(hook);
    cleanup(&mut ops, &state);
    assert!(ops.unlinks.is_empty());
}

#[test]
fn pid_contents_are_decimal_with_a_newline() {
    assert_eq!(pid_contents(1), b"1\n");
    assert_eq!(pid_contents(4242), b"4242\n");
    assert_eq!(pid_contents(i32::MAX), b"2147483647\n");
}

#[test]
fn second_pidfile_replaces_the_state() {
    let mut ops = FakePid::default();
    let mut state = PidState::new(hook);
    pidfile(&mut ops, Some(b"a"), b"p", &mut state).unwrap();
    pidfile(&mut ops, Some(b"b"), b"p", &mut state).unwrap();
    assert_eq!(state.path, b"/var/run/b.pid");
    assert_eq!(ops.writes.len(), 2);
    assert_eq!(ops.atexits.len(), 2);
}

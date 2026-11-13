// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `pty` tests over `FakePty`, adapted from `freebsd:lib/libutil/tests/forkpty_test.c`:
//! call order, the error unwinding and the `ENOSYS` fork.

#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented,
    clippy::field_reassign_with_default
)]

use super::*;
use crate::seams::{ForkResult, Termios};
use crate::testutil::FakePty;
use raven_libc_abi::errno::Errno;

#[test]
fn openpty_call_order() {
    let mut pty = FakePty::default();
    let (master, slave) = openpty(&mut pty, None, None, None).unwrap();
    assert_eq!(master, 3);
    assert_eq!(slave, 4);
    assert_eq!(
        pty.calls,
        alloc::vec![
            "posix_openpt".to_string(),
            "flags:258".to_string(),
            "grantpt".to_string(),
            "unlockpt".to_string(),
            "ptsname_r".to_string(),
            "open".to_string(),
            "/dev/pts/1".to_string(),
        ]
    );
}

#[test]
fn openpty_writes_the_name() {
    let mut pty = FakePty::default();
    let mut name = [0u8; 32];
    openpty(&mut pty, Some(&mut name), None, None).unwrap();
    assert_eq!(&name[..10], b"/dev/pts/1");
    assert_eq!(name[10], 0);
}

#[test]
fn openpty_name_buffer_too_small_closes_both() {
    let mut pty = FakePty::default();
    let mut name = [0u8; 3];
    assert_eq!(
        openpty(&mut pty, Some(&mut name), None, None),
        Err(Errno(raven_libc_abi::errno::ERANGE as u16))
    );
    assert_eq!(pty.closed, alloc::vec![3, 4]);
}

#[test]
fn each_failing_step_closes_the_master() {
    for step in ["grantpt", "unlockpt", "ptsname_r", "open"] {
        let mut pty = FakePty::default();
        pty.fail_at = Some(step);
        assert!(openpty(&mut pty, None, None, None).is_err(), "step {step}");
        assert_eq!(pty.closed, alloc::vec![3], "step {step}");
    }
}

#[test]
fn termios_and_winsize_are_set_on_the_slave() {
    let mut pty = FakePty::default();
    openpty(
        &mut pty,
        None,
        Some(&Termios::ZERO),
        Some(&Winsize::default()),
    )
    .unwrap();
    assert!(pty.calls.iter().any(|call| call == "tcsetattr"));
    assert!(pty.calls.iter().any(|call| call == "set_winsize"));
}

#[test]
fn login_tty_order() {
    let mut pty = FakePty::default();
    login_tty(&mut pty, 5).unwrap();
    assert_eq!(
        pty.calls,
        alloc::vec![
            "setsid".to_string(),
            "set_ctty".to_string(),
            "dup2".to_string(),
            "dup2:5:0".to_string(),
            "dup2".to_string(),
            "dup2:5:1".to_string(),
            "dup2".to_string(),
            "dup2:5:2".to_string(),
            "close:5".to_string(),
        ]
    );
}

#[test]
fn login_tty_does_not_close_a_standard_descriptor() {
    let mut pty = FakePty::default();
    login_tty(&mut pty, 2).unwrap();
    assert!(pty.closed.is_empty());
}

#[test]
fn login_tty_ctty_failure_is_returned() {
    let mut pty = FakePty::default();
    pty.fail_at = Some("set_ctty");
    assert!(login_tty(&mut pty, 5).is_err());
    assert!(pty.closed.is_empty());
}

#[test]
fn forkpty_returns_enosys_and_closes_both_ends() {
    let mut pty = FakePty::default();
    assert_eq!(
        forkpty(&mut pty, None, None, None),
        Err(Errno(raven_libc_abi::errno::ENOSYS as u16))
    );
    assert_eq!(pty.closed, alloc::vec![3, 4]);
}

#[test]
fn forkpty_parent_closes_the_slave() {
    let mut pty = FakePty::default();
    pty.fork_result = Ok(ForkResult::Parent(0));
    let (result, master) = forkpty(&mut pty, None, None, None).unwrap();
    assert_eq!(result, ForkResult::Parent(99));
    assert_eq!(master, 3);
    assert_eq!(pty.closed, alloc::vec![4]);
}

#[test]
fn forkpty_child_closes_the_master_and_logs_in() {
    let mut pty = FakePty::default();
    pty.fork_result = Ok(ForkResult::Child);
    let (result, master) = forkpty(&mut pty, None, None, None).unwrap();
    assert_eq!(result, ForkResult::Child);
    assert_eq!(master, -1);
    assert!(pty.closed.contains(&3));
    assert!(pty.calls.iter().any(|call| call == "set_ctty"));
}

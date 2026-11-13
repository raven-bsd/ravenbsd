// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::assertions_on_constants
)]

//! The `system` protocol against a recording backend.

use super::*;

#[derive(Debug, PartialEq)]
enum Call {
    Block(Vec<Sig>),
    Spawn(Vec<u8>),
    Ignore(Vec<Sig>),
    Restore,
    Wait(i32),
}

struct Fake {
    calls: Vec<Call>,
    spawn_error: Option<Errno>,
    wait: Vec<Result<i32, Errno>>,
}

impl Fake {
    fn new() -> Self {
        Self {
            calls: Vec::new(),
            spawn_error: None,
            wait: Vec::new(),
        }
    }
}

impl SystemBackend for Fake {
    fn block(&mut self, s: &[Sig]) {
        self.calls.push(Call::Block(s.to_vec()));
    }
    fn spawn_sh(&mut self, cmd: &[u8]) -> Result<i32, Errno> {
        self.calls.push(Call::Spawn(cmd.to_vec()));
        match self.spawn_error {
            Some(error) => Err(error),
            None => Ok(42),
        }
    }
    fn ignore(&mut self, s: &[Sig]) {
        self.calls.push(Call::Ignore(s.to_vec()));
    }
    fn unblock_and_restore(&mut self) {
        self.calls.push(Call::Restore);
    }
    fn wait(&mut self, pid: i32) -> Result<i32, Errno> {
        self.calls.push(Call::Wait(pid));
        if self.wait.is_empty() {
            Ok(0)
        } else {
            self.wait.remove(0)
        }
    }
}

fn errno(value: i32) -> Errno {
    Errno(value as u16)
}

#[test]
fn none_returns_one_without_touching_signals() {
    let mut fake = Fake::new();
    assert_eq!(system(&mut fake, None), 1);
    assert!(fake.calls.is_empty());
}

#[test]
fn spawn_precedes_ignore_and_restore_is_last() {
    let mut fake = Fake::new();
    fake.wait.push(Ok(7 << 8));
    assert_eq!(system(&mut fake, Some(b"echo hi")), 7 << 8);
    assert_eq!(
        fake.calls,
        vec![
            Call::Block(vec![Sig::Chld, Sig::Int, Sig::Quit]),
            Call::Spawn(b"echo hi".to_vec()),
            Call::Ignore(vec![Sig::Int, Sig::Quit]),
            Call::Restore,
            Call::Wait(42),
            Call::Restore,
        ]
    );
}

#[test]
fn eintr_is_retried() {
    let mut fake = Fake::new();
    fake.wait.push(Err(errno(EINTR)));
    fake.wait.push(Err(errno(EINTR)));
    fake.wait.push(Ok(3));
    assert_eq!(system(&mut fake, Some(b"true")), 3);
    let waits = fake
        .calls
        .iter()
        .filter(|call| matches!(call, Call::Wait(_)))
        .count();
    assert_eq!(waits, 3);
}

#[test]
fn spawn_failure_restores_and_returns_minus_one() {
    let mut fake = Fake::new();
    fake.spawn_error = Some(errno(raven_libc_abi::errno::ENOSYS));
    assert_eq!(system(&mut fake, Some(b"x")), -1);
    assert_eq!(
        fake.calls,
        vec![
            Call::Block(vec![Sig::Chld, Sig::Int, Sig::Quit]),
            Call::Spawn(b"x".to_vec()),
            Call::Restore,
        ]
    );
}

#[test]
fn wait_failure_returns_minus_one_after_restoring() {
    let mut fake = Fake::new();
    fake.wait.push(Err(errno(raven_libc_abi::errno::ECHILD)));
    assert_eq!(system(&mut fake, Some(b"x")), -1);
    assert_eq!(fake.calls.last(), Some(&Call::Restore));
}

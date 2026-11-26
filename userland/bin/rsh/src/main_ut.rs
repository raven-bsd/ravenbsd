// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the console read retry contract (S208).

use super::read_console_waiting;

/// Stand-in for the console error set; only equality matters to the retry policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TestErrno {
    /// The console server has no input ready.
    Again,
    /// A signal interrupted the read.
    Interrupted,
    /// The console backend failed.
    Io,
}

#[test]
fn idle_console_read_waits_for_input() {
    let attempts = [Err(TestErrno::Again), Err(TestErrno::Again), Ok(3)];
    let mut next = 0;
    let mut waits = 0;
    let mut buffer = [0u8; 8];
    let result = read_console_waiting(
        true,
        TestErrno::Again,
        |_| {
            let attempt = attempts[next];
            next += 1;
            attempt
        },
        || waits += 1,
        &mut buffer,
    );
    assert_eq!(result, Ok(3));
    assert_eq!(next, 3);
    assert_eq!(waits, 2);
}

#[test]
fn console_read_returns_other_errors_without_waiting() {
    for attempt in [Err(TestErrno::Interrupted), Err(TestErrno::Io)] {
        let mut reads = 0;
        let mut waits = 0;
        let mut buffer = [0u8; 8];
        let result = read_console_waiting(
            true,
            TestErrno::Again,
            |_| {
                reads += 1;
                attempt
            },
            || waits += 1,
            &mut buffer,
        );
        assert_eq!(result, attempt);
        assert_eq!(reads, 1);
        assert_eq!(waits, 0);
    }
}

#[test]
fn non_console_would_block_is_returned() {
    let mut reads = 0;
    let mut waits = 0;
    let mut buffer = [0u8; 8];
    let result = read_console_waiting(
        false,
        TestErrno::Again,
        |_| {
            reads += 1;
            Err(TestErrno::Again)
        },
        || waits += 1,
        &mut buffer,
    );
    assert_eq!(result, Err(TestErrno::Again));
    assert_eq!(reads, 1);
    assert_eq!(waits, 0);
}

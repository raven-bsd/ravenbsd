// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the process command-name helpers.

use super::*;
use alloc::vec;

#[test]
fn truncation_is_byte_bounded_and_keeps_utf8() {
    let text = "a".repeat(MAX_COMMAND + 20);
    assert_eq!(truncate_command(&text).len(), MAX_COMMAND);

    let wide = "é".repeat(MAX_COMMAND);
    let truncated = truncate_command(&wide);
    assert!(truncated.len() <= MAX_COMMAND);
    assert!(truncated.is_char_boundary(truncated.len()));
}

#[test]
fn argv_zero_wins_over_the_path() {
    let argv = vec![b"shell".to_vec(), b"-c".to_vec()];
    assert_eq!(command_from_argv(&argv, b"/bin/shell"), "shell");
}

#[test]
fn an_empty_argv_falls_back_to_the_path() {
    assert_eq!(command_from_argv(&[], b"/sbin/init"), "/sbin/init");
}

#[test]
fn invalid_utf8_falls_back_to_the_path() {
    let argv = vec![vec![0xff, 0xfe]];
    assert_eq!(command_from_argv(&argv, b"/bin/bad"), "/bin/bad");
}

#[test]
fn exec_reset_resets_caught_handlers_and_keeps_ignored() {
    let mut sigacts = SigActs::new();
    sigacts.set(
        status::SIGUSR1,
        SigAction {
            handler: 0x40_0000,
            mask: 1,
            flags: SA_SIGINFO,
        },
    );
    sigacts.set(
        12,
        SigAction {
            handler: SIG_IGN,
            mask: 0,
            flags: 0,
        },
    );
    assert!(sigacts.get(status::SIGUSR1).is_caught());
    sigacts.exec_reset();
    assert_eq!(sigacts.get(status::SIGUSR1), SigAction::default_action());
    assert_eq!(sigacts.get(12).handler, SIG_IGN);
}

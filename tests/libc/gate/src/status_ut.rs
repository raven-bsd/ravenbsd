// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Exit classification tests.

use super::*;

#[test]
fn pass_exit_is_met() {
    assert_eq!(from_exit(Some(0), 0, &[77]), Status::Met);
}

#[test]
fn failing_exit_is_not_met() {
    assert_eq!(from_exit(Some(3), 0, &[77]), Status::NotImplemented);
}

#[test]
fn blocked_exit_is_blocked() {
    assert_eq!(from_exit(Some(77), 0, &[77]), Status::Blocked);
}

#[test]
fn signal_or_timeout_is_blocked() {
    assert_eq!(from_exit(None, 0, &[77]), Status::Blocked);
}

#[test]
fn non_zero_pass_rule_is_honoured() {
    assert_eq!(from_exit(Some(5), 5, &[]), Status::Met);
    assert_eq!(from_exit(Some(0), 5, &[]), Status::NotImplemented);
}

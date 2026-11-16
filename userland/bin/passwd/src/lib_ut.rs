// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for `passwd` argument and password handling.

use super::*;

#[test]
fn the_default_action_changes_the_caller() {
    let options = parse_args([]).expect("parse");
    assert_eq!(options.action, Action::Change);
    assert_eq!(options.user, None);
}

#[test]
fn lock_and_unlock_parse() {
    assert_eq!(parse_args(["-l"]).expect("lock").action, Action::Lock);
    assert_eq!(
        parse_args(["-u", "alice"]).expect("unlock").action,
        Action::Unlock
    );
    assert_eq!(
        parse_args(["-l", "alice"]).expect("lock name").user,
        Some("alice")
    );
}

#[test]
fn conflicting_actions_are_rejected() {
    assert_eq!(
        parse_args(["-l", "-u"]).expect_err("conflict"),
        UsageError::Conflict
    );
    assert_eq!(
        parse_args(["-lu"]).expect_err("conflict"),
        UsageError::Conflict
    );
}

#[test]
fn operands_and_options_are_validated() {
    assert_eq!(
        parse_args(["alice", "bob"]).expect_err("extra"),
        UsageError::ExtraOperand
    );
    assert_eq!(
        parse_args(["-z"]).expect_err("unknown"),
        UsageError::UnknownOption('z')
    );
    assert_eq!(parse_args(["--", "-l"]).expect("literal").user, Some("-l"));
}

#[test]
fn root_may_set_a_short_root_password_only() {
    assert_eq!(minimum_length(0, 0), 1);
    assert_eq!(minimum_length(0, 1000), 8);
    assert_eq!(minimum_length(1000, 1000), 8);
    assert!(validate_password("x", 0, 0).is_ok());
    assert_eq!(
        validate_password("x", 0, 1000),
        Err(PasswordError::TooShort)
    );
    assert_eq!(
        validate_password("x", 1000, 1000),
        Err(PasswordError::TooShort)
    );
    assert!(validate_password("12345678", 0, 1000).is_ok());
}

#[test]
fn empty_and_line_broken_passwords_are_rejected() {
    assert_eq!(validate_password("", 0, 0), Err(PasswordError::Empty));
    assert_eq!(validate_password("a\0b", 0, 0), Err(PasswordError::Nul));
    assert_eq!(validate_password("a\nb", 0, 0), Err(PasswordError::Newline));
    assert_eq!(validate_password("a\rb", 0, 0), Err(PasswordError::Newline));
}

#[test]
fn non_root_callers_authenticate_and_stay_on_their_account() {
    assert!(!requires_old_password(0));
    assert!(requires_old_password(1000));
    assert!(target_allowed(0, 1000));
    assert!(target_allowed(1000, 1000));
    assert!(!target_allowed(1000, 1001));
}

#[test]
fn old_password_uses_raven_auth_and_refuses_locked_hashes() {
    let hash = raven_auth::hash_password("old-password", b"passwd-test").expect("hash");
    assert!(verify_old_password(&hash, "old-password"));
    assert!(!verify_old_password(&hash, "wrong-password"));
    assert!(!verify_old_password("!", "old-password"));
    assert!(!verify_old_password("", "old-password"));
}

#[test]
fn days_are_whole_and_non_negative() {
    assert_eq!(days_since_epoch(0), 0);
    assert_eq!(days_since_epoch(86_399), 0);
    assert_eq!(days_since_epoch(86_400), 1);
    assert_eq!(days_since_epoch(1_641_062_400), 18_993);
    assert_eq!(days_since_epoch(-1), 0);
}

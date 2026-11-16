// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `kill` signal table and parser.

use super::*;
use alloc::vec;

#[test]
fn names_and_numbers_round_trip() {
    assert_eq!(by_name("TERM"), Some(15));
    assert_eq!(by_name("sigterm"), Some(15));
    assert_eq!(by_name("9"), None);
    assert_eq!(by_number(15), Some("TERM"));
    assert_eq!(by_number(0), None);
}

#[test]
fn parse_signal_accepts_zero_names_and_numbers() {
    assert_eq!(parse_signal("0"), Some(0));
    assert_eq!(parse_signal("9"), Some(9));
    assert_eq!(parse_signal("KILL"), Some(9));
    assert_eq!(parse_signal("SIGKILL"), Some(9));
    assert_eq!(parse_signal("65"), None);
    assert_eq!(parse_signal("NOPE"), None);
}

#[test]
fn parses_list_without_and_with_query() {
    assert_eq!(parse_args(["-l"]).unwrap(), Action::List { query: None });
    assert_eq!(
        parse_args(["-l", "15"]).unwrap(),
        Action::List {
            query: Some("15".to_string())
        }
    );
}

#[test]
fn parses_default_and_explicit_signals() {
    assert_eq!(
        parse_args(["123"]).unwrap(),
        Action::Send {
            signal: 15,
            targets: vec![123]
        }
    );
    assert_eq!(
        parse_args(["-s", "KILL", "123"]).unwrap(),
        Action::Send {
            signal: 9,
            targets: vec![123]
        }
    );
    assert_eq!(
        parse_args(["-9", "123"]).unwrap(),
        Action::Send {
            signal: 9,
            targets: vec![123]
        }
    );
}

#[test]
fn parses_process_group_targets_after_double_dash() {
    assert_eq!(
        parse_args(["-0", "--", "-123"]).unwrap(),
        Action::Send {
            signal: 0,
            targets: vec![-123]
        }
    );
}

#[test]
fn rejects_bad_signal_and_missing_or_bad_targets() {
    assert_eq!(parse_args(["-s", "NOPE", "1"]), Err(UsageError::BadSignal));
    assert_eq!(parse_args([]), Err(UsageError::MissingTarget));
    assert_eq!(parse_args(["nope"]), Err(UsageError::BadTarget));
}

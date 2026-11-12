// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `ps` pure logic.

use super::*;
use std::string::String;

#[test]
fn empty_args_lists() {
    assert_eq!(parse_args([].into_iter()), Ok(Action::List));
}

#[test]
fn selftest_is_recognised() {
    assert_eq!(parse_args(["--selftest"].into_iter()), Ok(Action::Selftest));
}

#[test]
fn extra_or_unknown_arguments_are_rejected() {
    assert_eq!(parse_args(["x"].into_iter()), Err(UsageError));
    assert_eq!(parse_args(["--", "x"].into_iter()), Err(UsageError));
    assert_eq!(parse_args(["--selftest", "x"].into_iter()), Err(UsageError));
}

#[test]
fn every_state_formats() {
    for state in STATES {
        let line = format!("logd {state} 12 3\n");
        let mut out = String::new();
        format_status(&line, &mut out).unwrap();
        assert_eq!(out, format!("logd {state} 3\n"));
    }
}

#[test]
fn malformed_status_lines_are_rejected() {
    assert_eq!(format_status("", &mut String::new()), Err(StatusError));
    assert_eq!(
        format_status("logd running 1", &mut String::new()),
        Err(StatusError)
    );
    assert_eq!(
        format_status("logd dancing 1 2", &mut String::new()),
        Err(StatusError)
    );
    assert_eq!(
        format_status("logd running x 2", &mut String::new()),
        Err(StatusError)
    );
    assert_eq!(
        format_status("logd running 1 two", &mut String::new()),
        Err(StatusError)
    );
    assert_eq!(
        format_status("logd running 1 2 extra", &mut String::new()),
        Err(StatusError)
    );
}

#[test]
fn whitespace_is_tolerated() {
    let mut out = String::new();
    format_status("  ps   running   7   0  \n", &mut out).unwrap();
    assert_eq!(out, "ps running 0\n");
}

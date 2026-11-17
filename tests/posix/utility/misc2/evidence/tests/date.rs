// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Behaviour cases for `date`.

use raven_date::{Action, UsageError, format_uptime, parse_args};

fn render(seconds: u64) -> String {
    let mut out = String::new();
    format_uptime(seconds, &mut out).expect("date writes to String");
    out
}

#[test]
fn fixed_epoch_values_use_the_uptime_contract() {
    assert_eq!(render(0), "up 0s");
    assert_eq!(render(42), "up 42s");
    assert_eq!(render(1_700_000_000), "up 1700000000s");
}

#[test]
fn selftest_and_double_dash_are_accepted() {
    assert_eq!(parse_args([].into_iter()), Ok(Action::Print));
    assert_eq!(parse_args(["--"].into_iter()), Ok(Action::Print));
    assert_eq!(parse_args(["--selftest"].into_iter()), Ok(Action::Selftest));
}

#[test]
fn extra_arguments_are_a_usage_error() {
    assert_eq!(parse_args(["x"].into_iter()), Err(UsageError));
    assert_eq!(parse_args(["--selftest", "x"].into_iter()), Err(UsageError));
}

#[test]
fn posix_utc_and_format_options_are_not_implemented() {
    // P4z-D1: protocol version 1 has no wall clock, so the POSIX `-u` and `+format` forms are
    // usage errors. This case pins the defect instead of registering an unsupported behaviour.
    assert_eq!(parse_args(["-u"].into_iter()), Err(UsageError));
    assert_eq!(
        parse_args(["+%Y-%m-%dT%H:%M:%SZ"].into_iter()),
        Err(UsageError)
    );
}

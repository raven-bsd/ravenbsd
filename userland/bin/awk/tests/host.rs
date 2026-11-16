// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host end-to-end checks for the bounded awk core.

use raven_awk::{Options, run};

#[test]
fn report_and_field_session() {
    let output = run(
        "BEGIN { FS=\":\" } { total += $2 } END { print total, NR }",
        "a:2\nb:3\n",
        &Options::default(),
    )
    .unwrap();
    assert_eq!(output, "5 2\n");
}

#[test]
fn pattern_action_and_builtins_session() {
    let output = run(
        "$1 ~ /^a/ { print toupper($2), length($1) }",
        "alpha one\nbeta two\n",
        &Options::default(),
    )
    .unwrap();
    assert_eq!(output, "ONE 5\n");
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Table file parser tests.

use super::*;

#[test]
fn addresses_comments_and_negation_parse() {
    let file = parse_table_file(
        "# web hosts\n10.0.0.1\n!10.0.0.2   # exclude\n\n2001:db8::/32\n10.0.0.0/8\n",
    )
    .expect("parse");
    assert_eq!(file.entries.len(), 4);
    assert_eq!(file.entries[0].address, "10.0.0.1");
    assert!(file.entries[1].negative);
    assert_eq!(file.entries[1].address, "10.0.0.2");
    assert_eq!(file.entries[2].address, "2001:db8::/32");
    assert_eq!(file.entries[3].address, "10.0.0.0/8");
}

#[test]
fn malformed_lines_report_their_number() {
    let error = parse_table_file("10.0.0.1\nnonsense\n").expect_err("must fail");
    assert_eq!(error.line, 2);
    assert!(
        error.message.contains("invalid address `nonsense'"),
        "{}",
        error.message
    );
}

#[test]
fn extra_fields_are_rejected() {
    let error = parse_table_file("10.0.0.1 10.0.0.2\n").expect_err("must fail");
    assert_eq!(error.line, 1);
    assert!(error.message.contains("unexpected"), "{}", error.message);
}

#[test]
fn a_last_line_without_newline_is_read() {
    let file = parse_table_file("10.0.0.1").expect("parse");
    assert_eq!(file.entries.len(), 1);
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn parses_files_and_stop_option() {
    let options = parse_args(["-s", "a.c", "b.c"]).expect("options");
    assert!(options.first_only);
    assert_eq!(options.files, ["a.c", "b.c"]);
}

#[test]
fn rejects_missing_files() {
    assert_eq!(parse_args(["-s"]), Err(ParseError::MissingFile));
}

#[test]
fn rejects_unknown_options() {
    assert_eq!(parse_args(["-x", "a"]), Err(ParseError::UnknownOption));
}

#[test]
fn extracts_marker_until_quote() {
    let data = b"prefix @(#)module 1.2\" suffix @(#)next";
    assert_eq!(extract(data, false), ["@(#)module 1.2", "@(#)next"]);
}

#[test]
fn extracts_until_greater_than_and_newline_and_nul() {
    let data = b"@(#)a>@(#)b\n@(#)c\0@(#)d";
    assert_eq!(extract(data, false), ["@(#)a", "@(#)b", "@(#)c", "@(#)d"]);
}

#[test]
fn stop_after_first_string() {
    let data = b"@(#)one\" @(#)two\"";
    assert_eq!(extract(data, true), ["@(#)one"]);
}

#[test]
fn renders_one_tab_prefixed_line_each() {
    let strings = vec![String::from("@(#)one"), String::from("@(#)two")];
    assert_eq!(render(&strings), "\t@(#)one\n\t@(#)two\n");
}

#[test]
fn empty_input_has_no_strings() {
    assert!(extract(b"", false).is_empty());
    assert!(extract(b"no marker", false).is_empty());
}

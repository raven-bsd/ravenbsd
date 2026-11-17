// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn parses_options_and_files() {
    let options = parse_args(["-s", "-n", "-r", "1.2", "s.foo"]).expect("options");
    assert!(options.suppress);
    assert!(options.keep);
    assert_eq!(options.sid, Some("1.2"));
    assert_eq!(options.files, ["s.foo"]);
}

#[test]
fn rejects_missing_file() {
    assert_eq!(parse_args(["-s"]), Err(ParseError::MissingFile));
}

#[test]
fn rejects_unknown_option() {
    assert_eq!(parse_args(["-x", "s.foo"]), Err(ParseError::UnknownOption));
}

#[test]
fn rejects_missing_sid() {
    assert_eq!(parse_args(["-r"]), Err(ParseError::MissingSid));
}

#[test]
fn maps_sccs_names_to_p_files() {
    assert_eq!(pending_path("s.foo"), "p.foo");
    assert_eq!(pending_path("dir/s.foo"), "dir/p.foo");
    assert_eq!(pending_path("plain"), "p.plain");
}

#[test]
fn keeps_dot_names_intact() {
    assert_eq!(pending_path("s..x"), "p..x");
}

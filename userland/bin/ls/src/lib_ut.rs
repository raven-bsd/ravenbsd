// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `ls` argument and line formatting.

use super::{format_long, format_short, parse};

#[test]
fn default_has_no_flags() {
    let parsed = parse([]).unwrap();
    assert!(!parsed.flags.a && !parsed.flags.l);
    assert!(parsed.operands.is_empty());
}

#[test]
fn all_flag_is_parsed() {
    assert!(parse(["-a"]).unwrap().flags.a);
}

#[test]
fn long_flag_is_parsed() {
    assert!(parse(["-l"]).unwrap().flags.l);
}

#[test]
fn combined_long_and_human_flags_are_parsed() {
    let parsed = parse(["-lh"]).unwrap();
    assert!(parsed.flags.l && parsed.flags.h);
}

#[test]
fn unknown_flag_is_rejected() {
    assert!(parse(["-z"]).is_err());
}

#[test]
fn operands_are_kept() {
    assert_eq!(parse(["/", "/tmp"]).unwrap().operands, vec!["/", "/tmp"]);
}

#[test]
fn short_file_has_no_suffix() {
    assert_eq!(format_short("file", false), "file");
}

#[test]
fn short_directory_has_a_suffix() {
    assert_eq!(format_short("dir", true), "dir/");
}

#[test]
fn long_line_has_mode_size_and_name() {
    assert_eq!(format_long("file", 0o100644, 7), "-rw-r--r--        7 file");
}

#[test]
fn long_directory_line_has_no_suffix() {
    assert_eq!(format_long("dir", 0o040755, 0), "drwxr-xr-x        0 dir");
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for zcat argument handling.

use super::{Options, UsageError, parse_args, source_name};

#[test]
fn parses_files() {
    assert_eq!(
        parse_args(["a".into(), "b.Z".into()]).unwrap(),
        Options {
            files: vec!["a".into(), "b.Z".into()],
        }
    );
    assert_eq!(
        parse_args(Vec::<String>::new()),
        Err(UsageError::MissingFile)
    );
    assert_eq!(parse_args(["-x".into()]), Err(UsageError::UnknownOption));
}

#[test]
fn adds_the_z_suffix() {
    assert_eq!(source_name("a"), "a.Z");
    assert_eq!(source_name("a.Z"), "a.Z");
}

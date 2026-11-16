// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for uncompress argument handling.

use super::{Options, UsageError, output_name, parse_args};

#[test]
fn parses_stdout_and_files() {
    assert_eq!(
        parse_args(["-c".into(), "a.Z".into(), "b.Z".into()]).unwrap(),
        Options {
            stdout: true,
            files: vec!["a.Z".into(), "b.Z".into()],
        }
    );
    assert_eq!(
        parse_args(Vec::<String>::new()),
        Err(UsageError::MissingFile)
    );
    assert_eq!(
        parse_args(["-x".into(), "a.Z".into()]),
        Err(UsageError::UnknownOption)
    );
}

#[test]
fn strips_the_z_suffix() {
    assert_eq!(output_name("a.Z"), "a");
    assert_eq!(output_name("a"), "a");
}

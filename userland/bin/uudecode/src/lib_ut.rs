// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for uudecode options and output-name safety.

use super::{Options, UsageError, parse, safe_output_name};

#[test]
fn parses_output_and_input() {
    assert_eq!(
        parse(["-o", "out", "encoded"]).unwrap(),
        Options {
            output: Some("out"),
            input: Some("encoded"),
        }
    );
    assert_eq!(
        parse(["encoded"]).unwrap(),
        Options {
            output: None,
            input: Some("encoded"),
        }
    );
    assert_eq!(
        parse(Vec::<&str>::new()).unwrap(),
        Options {
            output: None,
            input: None,
        }
    );
}

#[test]
fn rejects_bad_options() {
    assert_eq!(parse(["-x"]), Err(UsageError::UnknownOption));
    assert_eq!(parse(["-o"]), Err(UsageError::MissingOutput));
    assert_eq!(parse(["a", "b"]), Err(UsageError::ExtraOperand));
}

#[test]
fn rejects_path_like_encoded_names() {
    assert!(safe_output_name("plain"));
    assert!(!safe_output_name(""));
    assert!(!safe_output_name("."));
    assert!(!safe_output_name(".."));
    assert!(!safe_output_name("../escape"));
    assert!(!safe_output_name("dir/file"));
}

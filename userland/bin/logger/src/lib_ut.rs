// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `logger` parser.

use super::*;
use alloc::vec;

#[test]
fn parses_named_and_numeric_priorities() {
    assert_eq!(parse_priority("user.info"), Some(14));
    assert_eq!(parse_priority("daemon.notice"), Some(29));
    assert_eq!(parse_priority("debug"), Some(15));
    assert_eq!(parse_priority("local0"), Some(133));
    assert_eq!(parse_priority("13"), Some(13));
    assert_eq!(parse_priority("999"), None);
    assert_eq!(parse_priority("nope.info"), None);
}

#[test]
fn parses_tag_file_and_message() {
    let options = parse_args(["-p", "user.info", "-t", "p4u", "-f", "/tmp/log", "hello"]).unwrap();
    assert_eq!(options.priority, 14);
    assert_eq!(options.tag.as_deref(), Some("p4u"));
    assert_eq!(options.file.as_deref(), Some("/tmp/log"));
    assert_eq!(options.message, vec!["hello"]);
}

#[test]
fn defaults_to_user_notice() {
    let options = parse_args(["hello", "world"]).unwrap();
    assert_eq!(options.priority, 13);
    assert_eq!(options.message, vec!["hello", "world"]);
}

#[test]
fn rejects_missing_values_messages_and_unknown_options() {
    assert_eq!(parse_args(["-p"]), Err(UsageError::MissingValue));
    assert_eq!(
        parse_args(["-p", "nope", "x"]),
        Err(UsageError::BadPriority)
    );
    assert_eq!(
        parse_args(Vec::<&str>::new()),
        Err(UsageError::MissingMessage)
    );
    assert_eq!(parse_args(["-x"]), Err(UsageError::UnknownOption));
}

#[test]
fn payload_prefixes_the_priority() {
    assert_eq!(payload(14, "hello"), "<14>hello");
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Header-field validation tests.

use super::{Header, valid_name, valid_value};

#[test]
fn accepts_the_http_token_alphabet() {
    for name in ["Host", "Content-Length", "X-Trace", "a!#$%&'*+-.^_`|~0"] {
        assert!(valid_name(name), "{name}");
    }
    for value in ["", "text/plain", "a b\tc", "~!@#$%^&*()"] {
        assert!(valid_value(value), "{value}");
    }
}

#[test]
fn rejects_controls_and_separators_in_names() {
    for name in [
        "",
        "Bad Name",
        "Bad:Name",
        "Bad\r\nName",
        "bad/name",
        "bad(name)",
    ] {
        assert!(!valid_name(name), "{name}");
    }
}

#[test]
fn rejects_crlf_in_values() {
    assert!(!valid_value("x\r\ny"));
    assert!(!valid_value("x\ny"));
    assert!(!valid_value("x\r"));
    assert!(!valid_value("x\0y"));
}

#[test]
fn builds_and_matches_case_insensitively() {
    let header = Header::new("Content-Length", "12").expect("header");
    assert!(header.is("content-length"));
    assert!(!header.is("content-type"));
    assert_eq!(header.value, "12");
    assert!(Header::new("Bad Name", "x").is_none());
}

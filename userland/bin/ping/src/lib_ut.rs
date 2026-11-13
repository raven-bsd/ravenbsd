// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `ping` argument parsing.

use super::*;

#[test]
fn defaults_apply_to_a_bare_host() {
    let options = parse(&["127.0.0.1"]).expect("defaults");
    assert_eq!(options.host, "127.0.0.1");
    assert_eq!(options.count, DEFAULT_COUNT);
    assert_eq!(options.timeout_secs, DEFAULT_TIMEOUT_SECS);
    assert_eq!(options.payload_len, DEFAULT_PAYLOAD);
}

#[test]
fn bounded_options_parse() {
    let options = parse(&["-c", "2", "-w", "3", "-s", "16", "10.0.0.1"]).expect("options");
    assert_eq!(options.count, 2);
    assert_eq!(options.timeout_secs, 3);
    assert_eq!(options.payload_len, 16);
}

#[test]
fn option_limits_are_enforced() {
    assert_eq!(parse(&["-c", "0", "127.0.0.1"]), Err(ParseError::BadCount));
    assert_eq!(
        parse(&["-c", "1001", "127.0.0.1"]),
        Err(ParseError::BadCount)
    );
    assert_eq!(
        parse(&["-w", "0", "127.0.0.1"]),
        Err(ParseError::BadTimeout)
    );
    assert_eq!(
        parse(&["-w", "61", "127.0.0.1"]),
        Err(ParseError::BadTimeout)
    );
    assert_eq!(
        parse(&["-s", "1401", "127.0.0.1"]),
        Err(ParseError::BadPayload)
    );
    assert_eq!(parse(&["-c", "01", "127.0.0.1"]), Err(ParseError::BadCount));
}

#[test]
fn malformed_operands_are_rejected() {
    assert_eq!(parse(&[]), Err(ParseError::Usage));
    assert_eq!(parse(&["-c"]), Err(ParseError::MissingValue));
    assert_eq!(parse(&["-z", "127.0.0.1"]), Err(ParseError::Usage));
    assert_eq!(parse(&["127.0.0.1", "127.0.0.2"]), Err(ParseError::Usage));
    assert_eq!(parse(&["300.0.0.1"]), Err(ParseError::BadAddress));
    assert_eq!(parse(&["127.0.0"]), Err(ParseError::BadAddress));
    assert_eq!(parse(&["127.0.0.1.2"]), Err(ParseError::BadAddress));
}

#[test]
fn address_validation_is_strict() {
    assert!(valid_ipv4("127.0.0.1"));
    assert!(valid_ipv4("255.255.255.255"));
    assert!(!valid_ipv4("127.00.0.1"));
    assert!(!valid_ipv4("127.0.0.-1"));
    assert!(!valid_ipv4(""));
    assert_eq!(parse_ipv4("10.1.2.3"), Ok([10, 1, 2, 3]));
}

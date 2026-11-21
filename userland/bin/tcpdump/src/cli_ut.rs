// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the command line.

use super::*;

fn parse_ok(arguments: &[&str]) -> Options {
    parse(arguments.iter().map(|value| String::from(*value)))
        .unwrap_or_else(|error| panic!("{arguments:?}: {error}"))
}

#[test]
fn parses_the_fixture_invocation() {
    let options = parse_ok(&["-nn", "-tt", "-r", "fixture.pcap", "tcp", "port", "80"]);
    assert_eq!(options.read.as_deref(), Some("fixture.pcap"));
    assert!(options.numeric);
    assert_eq!(options.timestamps, Timestamps::Unix);
    assert_eq!(options.filter.as_deref(), Some("tcp port 80"));
}

#[test]
fn parses_count_write_verbose_and_interface() {
    let options = parse_ok(&["-c", "3", "-w", "out.pcap", "-v", "-i", "lo"]);
    assert_eq!(options.count, Some(3));
    assert_eq!(options.write.as_deref(), Some("out.pcap"));
    assert_eq!(options.verbose, 1);
    assert_eq!(options.interface.as_deref(), Some("lo"));
}

#[test]
fn parses_attached_option_values() {
    let options = parse_ok(&["-rfixture.pcap", "-c5", "-vv", "-S"]);
    assert_eq!(options.read.as_deref(), Some("fixture.pcap"));
    assert_eq!(options.count, Some(5));
    assert_eq!(options.verbose, 2);
    assert!(options.absolute_seq);
}

#[test]
fn rejects_a_missing_option_value() {
    assert_eq!(
        parse([String::from("-r")]),
        Err(ParseError::MissingValue(String::from("-r")))
    );
}

#[test]
fn rejects_an_invalid_count() {
    assert_eq!(
        parse([String::from("-c"), String::from("0")]),
        Err(ParseError::InvalidCount(String::from("0")))
    );
}

#[test]
fn rejects_an_unknown_option() {
    assert!(matches!(
        parse([String::from("-Q")]),
        Err(ParseError::UnknownOption(_))
    ));
}

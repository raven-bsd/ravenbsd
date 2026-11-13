// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `nc` argument parsing.

use super::*;

#[test]
fn a_tcp_client_parses() {
    let options = parse(&["127.0.0.1", "7000"]).expect("tcp client");
    assert!(!options.listen);
    assert!(!options.udp);
    assert_eq!(options.host, Some("127.0.0.1"));
    assert_eq!(options.remote_port, Some(7000));
    assert_eq!(options.read_len, 0);
    assert_eq!(options.timeout_secs, 5);
}

#[test]
fn a_tcp_listener_parses() {
    let options = parse(&["-l", "-p", "7000", "-e"]).expect("listener");
    assert!(options.listen);
    assert!(options.echo);
    assert_eq!(options.port, Some(7000));
}

#[test]
fn a_udp_client_requires_a_bind_port() {
    assert_eq!(
        parse(&["-u", "127.0.0.1", "7001"]),
        Err(ParseError::MissingBind)
    );
    let options = parse(&["-u", "-b", "7102", "127.0.0.1", "7001"]).expect("udp client");
    assert!(options.udp);
    assert_eq!(options.bind, Some(7102));
}

#[test]
fn payload_and_length_options_parse() {
    let options = parse(&["-s", "hello", "-n", "5", "127.0.0.1", "7000"]).expect("options");
    assert_eq!(options.send, Some("hello"));
    assert_eq!(options.read_len, 5);
}

#[test]
fn malformed_options_are_rejected() {
    assert_eq!(parse(&["-l"]), Err(ParseError::MissingPort));
    assert_eq!(parse(&["-l", "-p", "0"]), Err(ParseError::BadPort));
    assert_eq!(
        parse(&["-l", "-p", "7000", "extra"]),
        Err(ParseError::Usage)
    );
    assert_eq!(
        parse(&["-w", "0", "127.0.0.1", "7000"]),
        Err(ParseError::BadTimeout)
    );
    assert_eq!(
        parse(&["-n", "1025", "127.0.0.1", "7000"]),
        Err(ParseError::BadLength)
    );
    assert_eq!(parse(&["300.1.1.1", "7000"]), Err(ParseError::BadAddress));
    assert_eq!(parse(&["127.0.0.1", "notaport"]), Err(ParseError::BadPort));
}

#[test]
fn address_and_port_validation() {
    assert!(valid_ipv4("127.0.0.1"));
    assert!(!valid_ipv4("127.0.0"));
    assert!(!valid_ipv4("127.0.0.1.2"));
    assert!(!valid_ipv4("127.00.0.1"));
    assert_eq!(parse_port("1"), Ok(1));
    assert_eq!(parse_port("65535"), Ok(65535));
    assert_eq!(parse_port("65536"), Err(ParseError::BadPort));
    assert_eq!(parse_port("01"), Err(ParseError::BadPort));
}

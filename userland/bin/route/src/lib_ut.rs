// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `route` argument parsing.

use super::*;

#[test]
fn empty_and_show_commands_list_the_table() {
    assert_eq!(parse(&[]), Ok(Command::List));
    assert_eq!(parse(&["show"]), Ok(Command::List));
    assert_eq!(parse(&["list"]), Ok(Command::List));
}

#[test]
fn a_default_route_needs_a_gateway() {
    assert_eq!(
        parse(&["add", "default", "198.51.100.1"]),
        Ok(Command::Add(RouteSpec {
            dest: [0; 4],
            prefix: 0,
            gateway: Some([198, 51, 100, 1]),
            iface: "lo",
        }))
    );
    assert_eq!(
        parse(&["add", "default", "198.51.100.1", "eth0"]),
        Ok(Command::Add(RouteSpec {
            dest: [0; 4],
            prefix: 0,
            gateway: Some([198, 51, 100, 1]),
            iface: "eth0",
        }))
    );
}

#[test]
fn host_and_network_routes_parse() {
    assert_eq!(
        parse(&["add", "host", "198.51.100.10"]),
        Ok(Command::Add(RouteSpec {
            dest: [198, 51, 100, 10],
            prefix: 32,
            gateway: None,
            iface: "lo",
        }))
    );
    assert_eq!(
        parse(&["add", "198.51.100.5/24"]),
        Ok(Command::Add(RouteSpec {
            dest: [198, 51, 100, 0],
            prefix: 24,
            gateway: None,
            iface: "lo",
        }))
    );
    assert_eq!(
        parse(&["add", "198.51.100.5"]),
        Ok(Command::Add(RouteSpec {
            dest: [198, 51, 100, 5],
            prefix: 32,
            gateway: None,
            iface: "lo",
        }))
    );
}

#[test]
fn delete_forms_parse() {
    assert_eq!(
        parse(&["delete", "default"]),
        Ok(Command::Delete {
            dest: [0; 4],
            prefix: 0,
            iface: "lo",
        })
    );
    assert_eq!(
        parse(&["del", "host", "198.51.100.10", "lo"]),
        Ok(Command::Delete {
            dest: [198, 51, 100, 10],
            prefix: 32,
            iface: "lo",
        })
    );
    assert_eq!(
        parse(&["delete", "198.51.100.5/24"]),
        Ok(Command::Delete {
            dest: [198, 51, 100, 0],
            prefix: 24,
            iface: "lo",
        })
    );
}

#[test]
fn malformed_commands_are_rejected() {
    assert_eq!(parse(&["add"]), Err(ParseError::Usage));
    assert_eq!(parse(&["add", "default"]), Err(ParseError::Usage));
    assert_eq!(
        parse(&["add", "host", "300.1.1.1"]),
        Err(ParseError::BadAddress)
    );
    assert_eq!(
        parse(&["add", "198.51.100.0/33"]),
        Err(ParseError::BadPrefix)
    );
    assert_eq!(
        parse(&["add", "host", "198.51.100.1", "bad name"]),
        Err(ParseError::BadInterface)
    );
    assert_eq!(parse(&["delete"]), Err(ParseError::Usage));
    assert_eq!(parse(&["frobnicate"]), Err(ParseError::Usage));
    assert_eq!(
        parse(&["add", "198.51.100.0/24", "lo", "extra"]),
        Err(ParseError::Usage)
    );
}

#[test]
fn destinations_are_canonicalised() {
    assert_eq!(canonical([198, 51, 100, 5], 24), [198, 51, 100, 0]);
    assert_eq!(canonical([198, 51, 100, 5], 32), [198, 51, 100, 5]);
    assert_eq!(canonical([198, 51, 100, 5], 0), [0, 0, 0, 0]);
    assert_eq!(parse_ipv4("10.1.2.3"), Ok([10, 1, 2, 3]));
    assert_eq!(flags_text(0), "-");
    assert_eq!(flags_text(7), "UP,GATEWAY,HOST");
}

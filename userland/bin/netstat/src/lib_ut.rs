// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `netstat` argument parsing and display names.

use super::*;

#[test]
fn defaults_show_sockets_and_counters() {
    assert_eq!(parse(&[]), Ok(Options::new()));
    assert!(Options::new().sockets);
    assert!(Options::new().counters);
}

#[test]
fn explicit_selections_are_respected() {
    assert_eq!(
        parse(&["-a"]),
        Ok(Options {
            sockets: true,
            counters: false,
        })
    );
    assert_eq!(
        parse(&["-i"]),
        Ok(Options {
            sockets: false,
            counters: true,
        })
    );
    assert_eq!(
        parse(&["-s"]),
        Ok(Options {
            sockets: false,
            counters: true,
        })
    );
    assert_eq!(
        parse(&["-a", "-i"]),
        Ok(Options {
            sockets: true,
            counters: true,
        })
    );
}

#[test]
fn unknown_options_are_rejected() {
    assert_eq!(parse(&["-x"]), Err(ParseError::Usage));
    assert_eq!(parse(&["extra"]), Err(ParseError::Usage));
    assert_eq!(parse(&["-h"]), Err(ParseError::Usage));
}

#[test]
fn names_cover_the_wire_ids() {
    assert_eq!(transport_text(1), "tcp");
    assert_eq!(transport_text(2), "udp");
    assert_eq!(transport_text(3), "icmp");
    assert_eq!(transport_text(0), "?");
    assert_eq!(state_text(0), "CLOSED");
    assert_eq!(state_text(1), "LISTEN");
    assert_eq!(state_text(4), "ESTABLISHED");
    assert_eq!(state_text(11), "BOUND");
    assert_eq!(state_text(12), "ECHO");
    assert_eq!(state_text(13), "?");
}

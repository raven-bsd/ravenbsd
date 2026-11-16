// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the thin wrapper's non-FFI helpers.

use super::{is_ipv4, nul};

#[test]
fn ipv4_literals_are_recognised() {
    assert!(is_ipv4("10.0.2.2"));
    assert!(is_ipv4("127.0.0.1"));
    assert!(is_ipv4("255.255.255.255"));
}

#[test]
fn dns_names_and_bad_literals_are_rejected() {
    assert!(!is_ipv4("localhost"));
    assert!(!is_ipv4("10.0.2"));
    assert!(!is_ipv4("10.0.2.256"));
    assert!(!is_ipv4("10.0.2.x"));
    assert!(!is_ipv4("10.0.2.2.5"));
}

#[test]
fn nul_terminates_and_limits_names() {
    let host = nul("10.0.2.2").expect("host fits");
    assert_eq!(host[8], 0);
    let long = "a".repeat(255);
    assert!(nul(&long).is_none());
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Address validation tests for the pf.conf AST.

use super::*;

#[test]
fn ipv4_accepts_dotted_quads_and_rejects_bad_octets() {
    assert!(is_ipv4("10.0.0.1"));
    assert!(is_ipv4("255.255.255.255"));
    assert!(is_ipv4("0.0.0.0"));
    assert!(!is_ipv4("256.0.0.1"));
    assert!(!is_ipv4("10.0.0"));
    assert!(!is_ipv4("10.0.0.1.2"));
    assert!(!is_ipv4("010.0.0.1"), "leading zero is ambiguous");
    assert!(!is_ipv4("10.0.0.x"));
}

#[test]
fn ipv6_accepts_full_and_compressed_forms() {
    assert!(is_ipv6("::1"));
    assert!(is_ipv6("::"));
    assert!(is_ipv6("2001:db8::1"));
    assert!(is_ipv6("fe80::1:2:3:4"));
    assert!(is_ipv6("2001:0db8:0000:0000:0000:0000:0000:0001"));
    assert!(!is_ipv6("2001:db8"));
    assert!(!is_ipv6("2001::db8::1"));
    assert!(!is_ipv6("gggg::1"));
    assert!(!is_ipv6("10.0.0.1"));
}

#[test]
fn address_literals_carry_valid_prefixes() {
    assert!(is_address_literal("10.0.0.0/8"));
    assert!(is_address_literal("2001:db8::/32"));
    assert!(is_address_literal("10.0.0.1/32"));
    assert!(is_address_literal("::1/128"));
    assert!(!is_address_literal("10.0.0.0/33"));
    assert!(!is_address_literal("2001:db8::/129"));
    assert!(!is_address_literal("10.0.0.0/"));
    assert!(!is_address_literal("10.0.0.0/x"));
}

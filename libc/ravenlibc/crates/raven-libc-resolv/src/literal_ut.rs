// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the numeric literal backend and the scope grammar.

use super::*;
use crate::consts::AF_UNSPEC;
use crate::types::Family;
use alloc::string::String;

struct Names;

impl IfNames for Names {
    fn name_to_index(&self, name: &str) -> Option<u32> {
        match name {
            "lo0" => Some(1),
            "eth0" => Some(7),
            _ => None,
        }
    }
    fn index_to_name(&self, idx: u32) -> Option<String> {
        match idx {
            1 => Some(String::from("lo0")),
            7 => Some(String::from("eth0")),
            _ => None,
        }
    }
}

fn names() -> Names {
    Names
}

fn v6(last: u8) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    bytes[15] = last;
    bytes
}

#[test]
fn a_dotted_quad_is_a_v4_literal() {
    let literal = lookup_ipliteral(b"127.0.0.1", AF_UNSPEC, &names())
        .expect("literal")
        .expect("some");
    assert_eq!(literal.family, Family::V4);
    assert_eq!(literal.ip(), crate::types::IpAddr::V4([127, 0, 0, 1]));
    assert_eq!(literal.scope, 0);
}

#[test]
fn a_legacy_short_quad_is_a_v4_literal() {
    let literal = lookup_ipliteral(b"1.2.3", AF_UNSPEC, &names())
        .expect("literal")
        .expect("some");
    assert_eq!(literal.ip(), crate::types::IpAddr::V4([1, 2, 0, 3]));
}

#[test]
fn an_ipv6_literal_is_accepted() {
    let literal = lookup_ipliteral(b"::1", AF_UNSPEC, &names())
        .expect("literal")
        .expect("some");
    assert_eq!(literal.family, Family::V6);
    assert_eq!(literal.addr, v6(1));
}

#[test]
fn a_non_numeric_name_is_none() {
    assert_eq!(
        lookup_ipliteral(b"example.org", AF_UNSPEC, &names()),
        Ok(None)
    );
}

#[test]
fn an_empty_name_is_none() {
    assert_eq!(lookup_ipliteral(b"", AF_UNSPEC, &names()), Ok(None));
}

#[test]
fn a_v4_literal_with_family_v6_is_nodata() {
    assert_eq!(
        lookup_ipliteral(b"127.0.0.1", crate::consts::AF_INET6, &names()),
        Err(Eai::NoData)
    );
}

#[test]
fn a_v6_literal_with_family_v4_is_nodata() {
    assert_eq!(
        lookup_ipliteral(b"::1", crate::consts::AF_INET, &names()),
        Err(Eai::NoData)
    );
}

#[test]
fn a_numeric_scope_is_parsed() {
    let literal = lookup_ipliteral(b"fe80::1%7", AF_UNSPEC, &names())
        .expect("literal")
        .expect("some");
    assert_eq!(literal.scope, 7);
}

#[test]
fn an_interface_scope_is_resolved_for_link_local() {
    let literal = lookup_ipliteral(b"fe80::1%eth0", AF_UNSPEC, &names())
        .expect("literal")
        .expect("some");
    assert_eq!(literal.scope, 7);
}

#[test]
fn an_interface_scope_is_resolved_for_link_local_multicast() {
    let literal = lookup_ipliteral(b"ff02::1%lo0", AF_UNSPEC, &names())
        .expect("literal")
        .expect("some");
    assert_eq!(literal.scope, 1);
}

#[test]
fn an_unknown_interface_scope_is_noname() {
    assert_eq!(
        lookup_ipliteral(b"fe80::1%nope", AF_UNSPEC, &names()),
        Err(Eai::NoName)
    );
}

#[test]
fn an_interface_scope_on_a_global_address_is_noname() {
    assert_eq!(
        lookup_ipliteral(b"2001:db8::1%lo0", AF_UNSPEC, &names()),
        Err(Eai::NoName)
    );
}

#[test]
fn a_scope_above_u32_is_noname() {
    assert_eq!(
        lookup_ipliteral(b"fe80::1%4294967296", AF_UNSPEC, &names()),
        Err(Eai::NoName)
    );
}

#[test]
fn a_scope_with_trailing_garbage_is_noname() {
    assert_eq!(
        lookup_ipliteral(b"fe80::1%12x", AF_UNSPEC, &names()),
        Err(Eai::NoName)
    );
}

#[test]
fn a_scope_without_a_digit_or_name_is_noname() {
    assert_eq!(
        lookup_ipliteral(b"fe80::1%", AF_UNSPEC, &names()),
        Err(Eai::NoName)
    );
}

#[test]
fn a_bad_v6_literal_is_none() {
    assert_eq!(lookup_ipliteral(b":::", AF_UNSPEC, &names()), Ok(None));
}

#[test]
fn is_numeric_accepts_both_families() {
    assert!(is_numeric(b"127.0.0.1"));
    assert!(is_numeric(b"::1"));
    assert!(!is_numeric(b"localhost"));
}

#[test]
fn a_scope_text_prefers_the_name() {
    assert_eq!(scope_text(7, &names(), false), "eth0");
    assert_eq!(scope_text(7, &names(), true), "7");
    assert_eq!(scope_text(99, &names(), false), "99");
}

#[test]
fn a_literal_v4_ip_is_the_original_quad() {
    let literal = lookup_ipliteral(b"192.0.2.1", AF_UNSPEC, &names())
        .expect("literal")
        .expect("some");
    assert_eq!(literal.ip(), crate::types::IpAddr::V4([192, 0, 2, 1]));
}

#[test]
fn a_percent_after_64_bytes_is_not_a_scope() {
    let mut name = alloc::vec![b'a'; 70];
    name[69] = b'%';
    assert_eq!(lookup_ipliteral(&name, AF_UNSPEC, &names()), Ok(None));
}

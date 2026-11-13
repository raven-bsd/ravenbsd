// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the `/etc/hosts` grammar and backend.

use super::*;
use crate::consts::{AF_INET, AF_INET6, AF_UNSPEC};
use crate::types::Family;
use alloc::string::String;

struct Names;

impl IfNames for Names {
    fn name_to_index(&self, name: &str) -> Option<u32> {
        if name == "eth0" { Some(7) } else { None }
    }
    fn index_to_name(&self, idx: u32) -> Option<String> {
        if idx == 7 {
            Some(String::from("eth0"))
        } else {
            None
        }
    }
}

fn names() -> Names {
    Names
}

fn lookup_text(file: &[u8], name: &str, family: i32) -> (alloc::vec::Vec<Address>, String, bool) {
    let mut canon = String::new();
    let (addrs, badfam) =
        lookup(file, name.as_bytes(), family, &names(), &mut canon).expect("lookup");
    (addrs, canon, badfam)
}

#[test]
fn a_simple_hosts_line_parses() {
    let entry = parse_hosts_line(b"127.0.0.1 localhost", &names()).expect("line");
    assert_eq!(entry.addr, IpAddr::V4([127, 0, 0, 1]));
    assert_eq!(entry.names, alloc::vec![String::from("localhost")]);
}

#[test]
fn aliases_are_kept_in_order() {
    let entry =
        parse_hosts_line(b"127.0.0.1 localhost localhost.localdomain", &names()).expect("line");
    assert_eq!(
        entry.names,
        alloc::vec![
            String::from("localhost"),
            String::from("localhost.localdomain")
        ]
    );
    assert_eq!(entry.canonical(), Some("localhost"));
}

#[test]
fn a_comment_ends_the_line() {
    let entry = parse_hosts_line(b"127.0.0.1 localhost # comment", &names()).expect("line");
    assert_eq!(entry.names.len(), 1);
}

#[test]
fn tabs_and_carriage_returns_are_separators() {
    let entry = parse_hosts_line(b"127.0.0.1\tlocalhost\r", &names()).expect("line");
    assert_eq!(entry.names, alloc::vec![String::from("localhost")]);
}

#[test]
fn an_ipv6_line_parses() {
    let entry = parse_hosts_line(b"::1 localhost", &names()).expect("line");
    assert_eq!(entry.addr.family(), Family::V6);
    assert_eq!(entry.addr, IpAddr::V6(crate::inet::IN6ADDR_LOOPBACK));
}

#[test]
fn an_ipv6_scope_is_kept() {
    let entry = parse_hosts_line(b"fe80::1%eth0 link", &names()).expect("line");
    assert_eq!(entry.scope, 7);
}

#[test]
fn a_line_without_a_name_is_rejected() {
    assert!(parse_hosts_line(b"127.0.0.1", &names()).is_none());
}

#[test]
fn an_empty_line_is_rejected() {
    assert!(parse_hosts_line(b"", &names()).is_none());
    assert!(parse_hosts_line(b"   # comment", &names()).is_none());
}

#[test]
fn a_bad_address_is_rejected() {
    assert!(parse_hosts_line(b"not-an-address name", &names()).is_none());
}

#[test]
fn parse_hosts_collects_every_line() {
    let file = b"127.0.0.1 a\n::1 b\n# comment\n192.0.2.1 c\n";
    let entries = parse_hosts(file, &names());
    assert_eq!(entries.len(), 3);
}

#[test]
fn an_over_long_line_is_skipped_whole() {
    let mut file = alloc::vec![b'x'; 600];
    file.extend_from_slice(b"\n127.0.0.1 ok\n");
    let entries = parse_hosts(&file, &names());
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].canonical(), Some("ok"));
}

#[test]
fn lookup_matches_case_insensitively() {
    let file = b"127.0.0.1 LocalHost\n";
    let (addrs, canon, badfam) = lookup_text(file, "localhost", AF_UNSPEC);
    assert_eq!(addrs.len(), 1);
    assert_eq!(canon, "LocalHost");
    assert!(!badfam);
}

#[test]
fn lookup_collects_all_matching_lines() {
    let file = b"127.0.0.1 host\n192.0.2.1 host\n";
    let (addrs, _, _) = lookup_text(file, "host", AF_UNSPEC);
    assert_eq!(addrs.len(), 2);
    assert_eq!(addrs[0].ip(), IpAddr::V4([127, 0, 0, 1]));
    assert_eq!(addrs[1].ip(), IpAddr::V4([192, 0, 2, 1]));
}

#[test]
fn lookup_caps_at_maxaddrs() {
    let mut file = alloc::vec::Vec::new();
    for index in 0..60u8 {
        let line = alloc::format!("10.0.0.{} host\n", index + 1);
        file.extend_from_slice(line.as_bytes());
    }
    let (addrs, _, _) = lookup_text(&file, "host", AF_UNSPEC);
    assert_eq!(addrs.len(), 48);
}

#[test]
fn lookup_honours_the_family() {
    let file = b"127.0.0.1 host\n::1 host\n";
    let (v4, _, _) = lookup_text(file, "host", AF_INET);
    assert_eq!(v4.len(), 1);
    assert_eq!(v4[0].family, Family::V4);
    let (v6, _, _) = lookup_text(file, "host", AF_INET6);
    assert_eq!(v6.len(), 1);
    assert_eq!(v6[0].family, Family::V6);
}

#[test]
fn a_wrong_family_only_match_is_badfam() {
    let file = b"::1 host\n";
    let (addrs, canon, badfam) = lookup_text(file, "host", AF_INET);
    assert!(addrs.is_empty());
    assert!(badfam);
    assert_eq!(canon, "host");
}

#[test]
fn lookup_stops_at_a_name_boundary() {
    let file = b"127.0.0.1 myhost\n";
    let (addrs, _, _) = lookup_text(file, "host", AF_UNSPEC);
    assert!(addrs.is_empty());
}

#[test]
fn lookup_uses_the_first_matching_line_for_the_canonical_name() {
    let file = b"127.0.0.1 first alias\n192.0.2.1 second\n";
    let (_, canon, _) = lookup_text(file, "alias", AF_UNSPEC);
    assert_eq!(canon, "first");
}

#[test]
fn an_invalid_canonical_name_is_not_used() {
    let file = b"127.0.0.1 bad_name\n";
    let (_, canon, _) = lookup_text(file, "bad_name", AF_UNSPEC);
    assert_eq!(canon, "");
}

#[test]
fn the_iterator_returns_entries_and_rewinds() {
    let file = b"127.0.0.1 a\n::1 b\n";
    let resolver = names();
    let mut iter = HostsIter::new(file, &resolver);
    assert_eq!(iter.next_entry().expect("a").canonical(), Some("a"));
    assert_eq!(iter.next_entry().expect("b").canonical(), Some("b"));
    assert!(iter.next_entry().is_none());
    iter.rewind();
    assert_eq!(iter.next_entry().expect("a").canonical(), Some("a"));
}

#[test]
fn the_gethostent_wrappers_work() {
    let file = b"127.0.0.1 a\n::1 b\n";
    let resolver = names();
    let mut state = sethostent(file, &resolver);
    assert_eq!(gethostent(&mut state).expect("a").canonical(), Some("a"));
    endhostent(&mut state);
    assert!(gethostent(&mut state).is_none());
    let mut again = sethostent(file, &resolver);
    assert_eq!(
        gethostent(&mut again).expect("again").canonical(),
        Some("a")
    );
}

#[test]
fn canonical_name_returns_the_first_match() {
    assert_eq!(
        canonical_name(b"127.0.0.1 first alias\n", b"alias", &names()).as_deref(),
        Some("first")
    );
}

#[test]
fn all_addresses_returns_both_families() {
    let addresses = all_addresses(b"127.0.0.1 x\n::1 x\n", b"x", &names());
    assert_eq!(addresses.len(), 2);
    assert_eq!(addresses[0].family(), Family::V4);
    assert_eq!(addresses[1].family(), Family::V6);
}

#[test]
fn the_netbsd_hosts_fixture_resolves_localhost_aliases() {
    let fixture = b"::1\t\tlocalhost localhost. localhost.localdomain.\n127.0.0.1\t\tlocalhost localhost. localhost.localdomain.\n";
    let (v6, canon, _) = lookup_text(fixture, "localhost.localdomain.", AF_INET6);
    assert_eq!(v6.len(), 1);
    assert_eq!(canon, "localhost");
    let (v4, _, _) = lookup_text(fixture, "LOCALHOST.", AF_INET);
    assert_eq!(v4.len(), 1);
    assert_eq!(v4[0].ip(), IpAddr::V4([127, 0, 0, 1]));
}

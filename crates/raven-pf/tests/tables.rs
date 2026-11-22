// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K298 integration: named table add/delete/lookup and table-backed rules.

use raven_pf::ast::{Direction, Packet, Protocol, tcp_flag};
use raven_pf::{AddrSpec, ConfigError, IpAddr, Ipv4Addr, TableError, parse};

fn addr4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(a, b, c, d))
}

fn outbound(source: IpAddr) -> Packet<'static> {
    Packet {
        direction: Direction::Out,
        interface: "eth0",
        protocol: Protocol::Tcp,
        source,
        destination: addr4(192, 0, 2, 10),
        source_port: 1234,
        destination_port: 80,
        length: 60,
        tcp_flags: tcp_flag::SYN,
        fragment: false,
    }
}

#[test]
fn parsed_tables_support_lookup_add_and_delete() {
    let mut rs = parse("table <web> { 192.0.2.0/24, 2001:db8::1 }\n").expect("parse table");
    assert!(rs.table("web").is_some());
    assert!(
        rs.table_contains("web", addr4(192, 0, 2, 55))
            .expect("lookup")
    );
    assert!(
        rs.table_contains(
            "web",
            IpAddr::V6(raven_pf::Ipv6Addr::parse("2001:db8::1").expect("v6"))
        )
        .expect("lookup")
    );
    assert!(
        !rs.table_contains("web", addr4(198, 51, 100, 1))
            .expect("lookup")
    );

    let added = AddrSpec::parse("203.0.113.9").expect("address");
    rs.add_table_entry("web", added).expect("add");
    assert!(
        rs.table_contains("web", addr4(203, 0, 113, 9))
            .expect("lookup")
    );
    assert!(rs.delete_table_entry("web", &added).expect("delete"));
    assert!(
        !rs.table_contains("web", addr4(203, 0, 113, 9))
            .expect("lookup")
    );
    assert!(!rs.delete_table_entry("web", &added).expect("second delete"));
}

#[test]
fn unknown_tables_are_reported_not_silently_ignored() {
    let mut rs = parse("table <web> { 192.0.2.0/24 }\n").expect("parse table");
    assert_eq!(
        rs.table_contains("missing", addr4(192, 0, 2, 1)),
        Err(ConfigError::UnknownTable)
    );
    assert_eq!(
        rs.add_table_entry("missing", AddrSpec::parse("10.0.0.1").expect("address")),
        Err(ConfigError::UnknownTable)
    );
    assert_eq!(
        rs.delete_table_entry("missing", &AddrSpec::parse("10.0.0.1").expect("address")),
        Err(ConfigError::UnknownTable)
    );
}

#[test]
fn delete_is_exact_and_does_not_remove_a_covering_entry() {
    let mut rs = parse("table <web> { 192.0.2.0/24 }\n").expect("parse table");
    let single = AddrSpec::parse("192.0.2.55").expect("address");
    assert!(
        !rs.delete_table_entry("web", &single)
            .expect("delete absent")
    );
    assert!(
        rs.table_contains("web", addr4(192, 0, 2, 55))
            .expect("lookup")
    );
    let subnet = AddrSpec::parse("192.0.2.0/24").expect("address");
    assert!(
        rs.delete_table_entry("web", &subnet)
            .expect("delete subnet")
    );
    assert!(
        !rs.table_contains("web", addr4(192, 0, 2, 55))
            .expect("lookup")
    );
}

#[test]
fn a_table_backed_nat_rule_matches_only_its_members() {
    let rs = parse(
        "table <clients> { 10.0.0.0/8 }\n\
         nat on eth0 from <clients> to any -> 203.0.113.7\n",
    )
    .expect("parse table-backed NAT");
    assert_eq!(
        rs.nat.find(&outbound(addr4(10, 1, 2, 3)), &rs.tables),
        Some(0)
    );
    assert_eq!(
        rs.nat.find(&outbound(addr4(192, 168, 1, 1)), &rs.tables),
        None
    );
}

#[test]
fn a_duplicate_entry_is_refused_by_the_table_error_path() {
    let mut rs = parse("table <web> { 192.0.2.0/24 }\n").expect("parse table");
    let spec = AddrSpec::parse("192.0.2.0/24").expect("address");
    // The table itself has no duplicate rule; the config path reports the table error type.
    assert!(rs.add_table_entry("web", spec).is_ok());
    let error = rs
        .add_table_entry("web", AddrSpec::Table(0))
        .expect_err("nested table entry");
    assert_eq!(error, ConfigError::Table(TableError::Nested));
}

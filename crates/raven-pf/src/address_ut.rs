// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for address and port specifications.

use super::*;
use crate::table::Table;

const A: Ipv4Addr = Ipv4Addr::new(10, 0, 0, 1);

#[test]
fn ipv4_round_trip_and_display() {
    for text in ["0.0.0.0", "10.0.0.1", "192.0.2.255", "255.255.255.255"] {
        let address = Ipv4Addr::parse(text).unwrap();
        assert_eq!(address.to_string(), text);
        assert_eq!(Ipv4Addr::from_u32(address.to_u32()), address);
    }
}

#[test]
fn ipv4_rejects_malformed_input() {
    for text in [
        "",
        "1.2.3",
        "1.2.3.4.5",
        "256.0.0.1",
        "1.2.3.a",
        "01.2.3.4",
        "1..3.4",
    ] {
        assert!(Ipv4Addr::parse(text).is_err(), "{text}");
    }
}

#[test]
fn ipv6_round_trip_and_canonical_display() {
    for (text, canonical) in [
        ("::", "::"),
        ("::1", "::1"),
        ("2001:db8::1", "2001:db8::1"),
        ("2001:0DB8:0000:0000:0000:0000:0000:0001", "2001:db8::1"),
        ("fe80::1", "fe80::1"),
        ("2001:db8:0:0:1:0:0:1", "2001:db8::1:0:0:1"),
        ("1:2:3:4:5:6:7:8", "1:2:3:4:5:6:7:8"),
    ] {
        let address = Ipv6Addr::parse(text).unwrap();
        assert_eq!(address.to_string(), canonical, "{text}");
        assert_eq!(Ipv6Addr::parse(&address.to_string()).unwrap(), address);
    }
}

#[test]
fn ipv6_segments_and_octets_are_network_order() {
    let address = Ipv6Addr::parse("2001:db8::1").unwrap();
    assert_eq!(address.segments()[0], 0x2001);
    assert_eq!(address.segments()[1], 0x0db8);
    assert_eq!(address.segments()[7], 1);
    assert_eq!(address.octets()[0], 0x20);
    assert_eq!(address.octets()[15], 1);
    assert_eq!(Ipv6Addr::from_segments(address.segments()), address);
}

#[test]
fn ipv6_rejects_malformed_input() {
    for text in [
        "",
        ":",
        ":::",
        "1::2::3",
        "1:2:3:4:5:6:7",
        "1:2:3:4:5:6:7:8:9",
        "12345::",
        "::ffff:10.0.0.1",
        "2001:db8::g",
        "2001:db8::/64",
    ] {
        assert!(Ipv6Addr::parse(text).is_err(), "{text}");
    }
}

#[test]
fn ip_addr_parses_both_families() {
    assert_eq!(IpAddr::parse("10.0.0.1").unwrap(), IpAddr::V4(A));
    assert!(IpAddr::parse("2001:db8::1").unwrap().is_v6());
    assert_eq!(
        IpAddr::parse("2001:db8::1").unwrap().to_string(),
        "2001:db8::1"
    );
    assert!(IpAddr::parse("not-an-address").is_err());
}

#[test]
fn address_host_and_any_match() {
    let host = AddrSpec::parse("10.0.0.5").unwrap();
    assert!(host.matches(Ipv4Addr::new(10, 0, 0, 5), &[]));
    assert!(!host.matches(Ipv4Addr::new(10, 0, 0, 6), &[]));
    let any = AddrSpec::parse("any").unwrap();
    assert!(any.matches(Ipv4Addr::new(203, 0, 113, 1), &[]));
    assert!(any.matches(Ipv6Addr::parse("2001:db8::1").unwrap(), &[]));
}

#[test]
fn cidr_matches_only_inside_the_prefix() {
    let net = AddrSpec::parse("10.1.2.0/23").unwrap();
    assert!(net.matches(Ipv4Addr::new(10, 1, 2, 1), &[]));
    assert!(net.matches(Ipv4Addr::new(10, 1, 3, 254), &[]));
    assert!(!net.matches(Ipv4Addr::new(10, 1, 4, 1), &[]));
    assert!(!net.matches(Ipv4Addr::new(10, 0, 255, 1), &[]));
}

#[test]
fn cidr_zero_and_host_prefixes() {
    let all = AddrSpec::parse("0.0.0.0/0").unwrap();
    assert!(all.matches(Ipv4Addr::new(8, 8, 8, 8), &[]));
    let one = AddrSpec::parse("192.0.2.7/32").unwrap();
    assert!(one.matches(Ipv4Addr::new(192, 0, 2, 7), &[]));
    assert!(!one.matches(Ipv4Addr::new(192, 0, 2, 8), &[]));
}

#[test]
fn cidr_is_normalized() {
    let net = AddrSpec::parse("10.1.3.9/23").unwrap();
    assert_eq!(
        net,
        AddrSpec::Network {
            network: IpAddr::V4(Ipv4Addr::new(10, 1, 2, 0)),
            prefix: 23,
        }
    );
    let v6 = AddrSpec::parse("2001:db8:1::5/32").unwrap();
    assert_eq!(
        v6,
        AddrSpec::Network {
            network: IpAddr::parse("2001:db8::").unwrap(),
            prefix: 32,
        }
    );
}

#[test]
fn ipv6_prefixes_match_by_network() {
    let net = AddrSpec::parse("2001:db8::/32").unwrap();
    assert!(net.matches(Ipv6Addr::parse("2001:db8:1::1").unwrap(), &[]));
    assert!(net.matches(Ipv6Addr::parse("2001:db8:ffff::").unwrap(), &[]));
    assert!(!net.matches(Ipv6Addr::parse("2001:db9::1").unwrap(), &[]));
    assert!(!net.matches(Ipv4Addr::new(10, 0, 0, 1), &[]));
    let host = AddrSpec::parse("2001:db8::7/128").unwrap();
    assert!(host.matches(Ipv6Addr::parse("2001:db8::7").unwrap(), &[]));
    assert!(!host.matches(Ipv6Addr::parse("2001:db8::8").unwrap(), &[]));
}

#[test]
fn bad_prefixes_are_rejected() {
    for text in [
        "10.0.0.0/33",
        "10.0.0.0/",
        "10.0.0.0/x",
        "10.0.0.0/999",
        "2001:db8::/129",
        "2001:db8::/",
    ] {
        assert!(AddrSpec::parse(text).is_err(), "{text}");
    }
}

#[test]
fn table_specs_match_member_addresses() {
    let mut table = Table::new("trusted");
    table.add(AddrSpec::parse("10.0.0.0/8").unwrap()).unwrap();
    table
        .add(AddrSpec::parse("2001:db8::/32").unwrap())
        .unwrap();
    let tables = [table];
    let reference = AddrSpec::Table(0);
    assert!(reference.matches(Ipv4Addr::new(10, 9, 8, 7), &tables));
    assert!(reference.matches(Ipv6Addr::parse("2001:db8::5").unwrap(), &tables));
    assert!(!reference.matches(Ipv4Addr::new(192, 0, 2, 1), &tables));
    assert!(!AddrSpec::Table(1).matches(Ipv4Addr::new(10, 0, 0, 1), &tables));
}

#[test]
fn port_equality_and_ranges() {
    let port = PortSpec::parse("22").unwrap();
    assert!(port.matches(22));
    assert!(!port.matches(23));
    let range = PortSpec::parse("1024:65535").unwrap();
    assert!(range.matches(1024));
    assert!(range.matches(65535));
    assert!(!range.matches(1023));
    assert!(PortSpec::parse(":1024").unwrap().matches(0));
    assert!(PortSpec::parse("1024:").unwrap().matches(65535));
    assert_eq!(PortSpec::parse("22").unwrap().span(), Some((22, 22)));
    assert_eq!(PortSpec::parse("20:30").unwrap().span(), Some((20, 30)));
}

#[test]
fn port_comparisons_and_negation() {
    assert!(PortSpec::parse("<1024").unwrap().matches(80));
    assert!(!PortSpec::parse("<1024").unwrap().matches(1024));
    assert!(PortSpec::parse(">1024").unwrap().matches(1025));
    assert!(PortSpec::parse("!=22").unwrap().matches(23));
    assert!(!PortSpec::parse("!=22").unwrap().matches(22));
    assert!(PortSpec::parse("any").unwrap().matches(1));
}

#[test]
fn invalid_ports_are_rejected() {
    for text in ["", "65536", "1:2:3", "1:-2", "<x", "1024:1023"] {
        assert!(PortSpec::parse(text).is_err(), "{text}");
    }
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for address and port specifications.

use super::*;

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
fn address_host_and_any_match() {
    let host = AddrSpec::parse("10.0.0.5").unwrap();
    assert!(host.matches(Ipv4Addr::new(10, 0, 0, 5)));
    assert!(!host.matches(Ipv4Addr::new(10, 0, 0, 6)));
    let any = AddrSpec::parse("any").unwrap();
    assert!(any.matches(Ipv4Addr::new(203, 0, 113, 1)));
}

#[test]
fn cidr_matches_only_inside_the_prefix() {
    let net = AddrSpec::parse("10.1.2.0/23").unwrap();
    assert!(net.matches(Ipv4Addr::new(10, 1, 2, 1)));
    assert!(net.matches(Ipv4Addr::new(10, 1, 3, 254)));
    assert!(!net.matches(Ipv4Addr::new(10, 1, 4, 1)));
    assert!(!net.matches(Ipv4Addr::new(10, 0, 255, 1)));
}

#[test]
fn cidr_zero_and_host_prefixes() {
    let all = AddrSpec::parse("0.0.0.0/0").unwrap();
    assert!(all.matches(Ipv4Addr::new(8, 8, 8, 8)));
    let one = AddrSpec::parse("192.0.2.7/32").unwrap();
    assert!(one.matches(Ipv4Addr::new(192, 0, 2, 7)));
    assert!(!one.matches(Ipv4Addr::new(192, 0, 2, 8)));
}

#[test]
fn cidr_is_normalized() {
    let net = AddrSpec::parse("10.1.3.9/23").unwrap();
    assert_eq!(
        net,
        AddrSpec::Network {
            network: Ipv4Addr::new(10, 1, 2, 0),
            prefix: 23,
        }
    );
}

#[test]
fn bad_prefixes_are_rejected() {
    for text in ["10.0.0.0/33", "10.0.0.0/", "10.0.0.0/x", "10.0.0.0/999"] {
        assert!(AddrSpec::parse(text).is_err(), "{text}");
    }
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

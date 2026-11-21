// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for rule matching.

use super::*;
use crate::address::{AddrSpec, Ipv4Addr, Ipv6Addr, PortSpec};

fn rule(action: Action, direction: Option<Direction>) -> Rule {
    Rule {
        action,
        quick: false,
        direction,
        interface: IfaceSpec::Any,
        family: None,
        protocol: Protocol::Any,
        source: AddrPort::any(),
        destination: AddrPort::any(),
        state: StatePolicy::None,
        log: false,
    }
}

fn packet<'a>(direction: Direction, interface: &'a str, protocol: Protocol) -> Packet<'a> {
    Packet {
        direction,
        interface,
        protocol,
        source: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
        destination: IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)),
        source_port: 12345,
        destination_port: 80,
        length: 60,
        tcp_flags: tcp_flag::SYN,
        fragment: false,
    }
}

fn v6_packet<'a>() -> Packet<'a> {
    Packet::tcp(
        Direction::In,
        "eth0",
        Ipv6Addr::parse("2001:db8::1").unwrap(),
        12345,
        Ipv6Addr::parse("2001:db8::2").unwrap(),
        443,
        tcp_flag::SYN,
        60,
    )
}

#[test]
fn direction_none_matches_both_ways() {
    let rule = rule(Action::Pass, None);
    assert!(rule.matches(&packet(Direction::In, "eth0", Protocol::Tcp), &[]));
    assert!(rule.matches(&packet(Direction::Out, "eth0", Protocol::Tcp), &[]));
}

#[test]
fn direction_restricts_matching() {
    let rule = rule(Action::Pass, Some(Direction::In));
    assert!(rule.matches(&packet(Direction::In, "eth0", Protocol::Tcp), &[]));
    assert!(!rule.matches(&packet(Direction::Out, "eth0", Protocol::Tcp), &[]));
}

#[test]
fn interface_restricts_matching() {
    let mut rule = rule(Action::Pass, None);
    rule.interface = IfaceSpec::Name("eth0".to_string());
    assert!(rule.matches(&packet(Direction::In, "eth0", Protocol::Tcp), &[]));
    assert!(!rule.matches(&packet(Direction::In, "lo", Protocol::Tcp), &[]));
    rule.interface = IfaceSpec::Any;
    assert!(rule.matches(&packet(Direction::In, "lo", Protocol::Tcp), &[]));
}

#[test]
fn protocol_restricts_matching() {
    let mut rule = rule(Action::Block, None);
    rule.protocol = Protocol::Udp;
    assert!(rule.matches(&packet(Direction::In, "eth0", Protocol::Udp), &[]));
    assert!(!rule.matches(&packet(Direction::In, "eth0", Protocol::Tcp), &[]));
    rule.protocol = Protocol::Any;
    assert!(rule.matches(&packet(Direction::In, "eth0", Protocol::Icmp), &[]));
    rule.protocol = Protocol::Icmpv6;
    assert!(!rule.matches(&packet(Direction::In, "eth0", Protocol::Icmp), &[]));
}

#[test]
fn the_address_family_condition_filters() {
    let mut rule = rule(Action::Pass, None);
    rule.family = Some(Family::Inet);
    assert!(rule.matches(&packet(Direction::In, "eth0", Protocol::Tcp), &[]));
    assert!(!rule.matches(&v6_packet(), &[]));
    rule.family = Some(Family::Inet6);
    assert!(rule.matches(&v6_packet(), &[]));
    assert!(!rule.matches(&packet(Direction::In, "eth0", Protocol::Tcp), &[]));
}

#[test]
fn source_and_destination_addresses_match() {
    let mut rule = rule(Action::Pass, None);
    rule.source.address = AddrSpec::parse("10.0.0.0/24").unwrap();
    rule.destination.address = AddrSpec::parse("192.0.2.1").unwrap();
    assert!(rule.matches(&packet(Direction::In, "eth0", Protocol::Tcp), &[]));
    rule.source.address = AddrSpec::parse("10.0.1.0/24").unwrap();
    assert!(!rule.matches(&packet(Direction::In, "eth0", Protocol::Tcp), &[]));
}

#[test]
fn ipv6_addresses_match() {
    let mut rule = rule(Action::Pass, None);
    rule.source.address = AddrSpec::parse("2001:db8::/32").unwrap();
    rule.destination.port = PortSpec::Equal(443);
    assert!(rule.matches(&v6_packet(), &[]));
    let other = Packet::tcp(
        Direction::In,
        "eth0",
        Ipv6Addr::parse("2001:db9::1").unwrap(),
        1,
        Ipv6Addr::parse("2001:db8::2").unwrap(),
        443,
        tcp_flag::SYN,
        60,
    );
    assert!(!rule.matches(&other, &[]));
}

#[test]
fn ports_match_tcp_and_udp() {
    let mut rule = rule(Action::Pass, None);
    rule.protocol = Protocol::Tcp;
    rule.destination.port = PortSpec::Equal(80);
    assert!(rule.matches(&packet(Direction::In, "eth0", Protocol::Tcp), &[]));
    assert!(!rule.matches(&packet(Direction::In, "eth0", Protocol::Udp), &[]));
    rule.protocol = Protocol::Udp;
    rule.destination.port = PortSpec::Range { start: 53, end: 53 };
    let mut udp = packet(Direction::In, "eth0", Protocol::Udp);
    udp.destination_port = 53;
    assert!(rule.matches(&udp, &[]));
}

#[test]
fn a_port_condition_never_matches_icmp() {
    let mut rule = rule(Action::Pass, None);
    rule.destination.port = PortSpec::Any;
    rule.source.port = PortSpec::Equal(1);
    assert!(!rule.matches(&packet(Direction::In, "eth0", Protocol::Icmp), &[]));
}

#[test]
fn fragments_never_match_port_conditions() {
    // Donor pf_match_rule(): PF_VPROTO_FRAGMENT fails a rule that names a
    // source or destination port (`PF_TEST_ATTRIB((r->src.port_op ||
    // r->dst.port_op), fail)`). A fragment can match an address-only rule.
    let mut port_rule = rule(Action::Pass, None);
    port_rule.protocol = Protocol::Tcp;
    port_rule.destination.port = PortSpec::Equal(443);
    let mut fragment = packet(Direction::In, "eth0", Protocol::Tcp);
    fragment.fragment = true;
    fragment.destination_port = 0;
    assert!(!port_rule.matches(&fragment, &[]));
    let mut plain = packet(Direction::In, "eth0", Protocol::Tcp);
    plain.destination_port = 0;
    assert!(!port_rule.matches(&plain, &[]));

    let mut address_only = rule(Action::Pass, None);
    address_only.protocol = Protocol::Tcp;
    address_only.destination.address = AddrSpec::parse("192.0.2.0/24").unwrap();
    assert!(address_only.matches(&fragment, &[]));
    assert!(address_only.matches(&plain, &[]));
}

#[test]
fn empty_ruleset_defaults_to_block() {
    let ruleset = Ruleset::new();
    assert_eq!(ruleset.default_action, Action::Block);
    assert!(ruleset.rules.is_empty());
    assert!(ruleset.tables.is_empty());
    assert!(ruleset.nat.is_empty());
}

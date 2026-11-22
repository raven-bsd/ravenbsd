// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K298 acceptance tests: NAT/RDR translation over IPv4 and IPv6, the pool port range,
//! sticky addresses and the redirect port formula.

use raven_pf::ast::tcp_flag;
use raven_pf::nat::remap_redirect_port;
use raven_pf::{
    AddrPort, Direction, IfaceSpec, IpAddr, NatAction, NatError, NatRule, NatTable, Packet, Pool,
    PoolKind, PortSpec, Protocol,
};

fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
    IpAddr::parse(&format!("{a}.{b}.{c}.{d}")).unwrap()
}

fn v6(text: &str) -> IpAddr {
    IpAddr::parse(text).unwrap()
}

fn rule(
    action: NatAction,
    kind: PoolKind,
    addresses: Vec<IpAddr>,
    proxy_port: Option<(u16, u16)>,
    sticky: bool,
) -> NatRule {
    NatRule {
        action,
        direction: Some(match action {
            NatAction::Nat => Direction::Out,
            NatAction::Rdr => Direction::In,
        }),
        interface: IfaceSpec::Any,
        protocol: Protocol::Any,
        source: AddrPort::any(),
        destination: AddrPort::any(),
        pool: Pool {
            kind,
            addresses,
            proxy_port,
            static_port: false,
            sticky,
        },
        log: false,
    }
}

#[test]
fn ipv4_nat_allocates_the_configured_port_range() {
    let nat_ip = v4(203, 0, 113, 9);
    let mut table = NatTable::new();
    assert!(table.push(rule(
        NatAction::Nat,
        PoolKind::None,
        vec![nat_ip],
        Some((40000, 40001)),
        false,
    )));
    let packet = Packet::tcp(
        Direction::Out,
        "eth0",
        v4(10, 0, 0, 5),
        1234,
        v4(192, 0, 2, 10),
        80,
        tcp_flag::SYN,
        60,
    );
    assert_eq!(table.find(&packet, &[]), Some(0));
    let first = table.translate(0, &packet, |_, _| false).unwrap().unwrap();
    assert_eq!(first.source, nat_ip);
    assert_eq!(first.source_port, 40000);
    assert_eq!(first.destination, v4(192, 0, 2, 10));
    assert_eq!(first.destination_port, 80);
    let second = table.translate(0, &packet, |_, _| false).unwrap().unwrap();
    assert_eq!(second.source_port, 40001);
    assert_eq!(
        table.translate(0, &packet, |_, _| true),
        Err(NatError::PoolExhausted)
    );
}

#[test]
fn ipv6_nat_uses_the_v6_pool_and_keeps_the_family() {
    let nat_ip = v6("2001:db8:ffff::1");
    let mut table = NatTable::new();
    assert!(table.push(rule(
        NatAction::Nat,
        PoolKind::None,
        vec![nat_ip],
        Some((40000, 40000)),
        false,
    )));
    let packet = Packet::tcp(
        Direction::Out,
        "eth0",
        v6("2001:db8::5"),
        1234,
        v6("2001:db8::10"),
        80,
        tcp_flag::SYN,
        60,
    );
    let translation = table.translate(0, &packet, |_, _| false).unwrap().unwrap();
    assert_eq!(translation.source, nat_ip);
    assert!(!translation.source.is_v4());
    assert_eq!(translation.source_port, 40000);
    assert_eq!(translation.destination, v6("2001:db8::10"));
}

#[test]
fn sticky_addresses_are_stable_per_source() {
    let first = v4(203, 0, 113, 1);
    let second = v4(203, 0, 113, 2);
    let mut table = NatTable::new();
    assert!(table.push(rule(
        NatAction::Nat,
        PoolKind::RoundRobin,
        vec![first, second],
        Some((50001, 50001)),
        true,
    )));
    let a = Packet::tcp(
        Direction::Out,
        "eth0",
        v4(10, 0, 0, 5),
        1234,
        v4(192, 0, 2, 10),
        80,
        tcp_flag::SYN,
        60,
    );
    let b = Packet::tcp(
        Direction::Out,
        "eth0",
        v4(10, 0, 0, 6),
        1234,
        v4(192, 0, 2, 10),
        80,
        tcp_flag::SYN,
        60,
    );
    let a1 = table.translate(0, &a, |_, _| false).unwrap().unwrap();
    let a2 = table.translate(0, &a, |_, _| false).unwrap().unwrap();
    let b1 = table.translate(0, &b, |_, _| false).unwrap().unwrap();
    assert_eq!(a1.source, a2.source);
    assert_ne!(a1.source, b1.source);
}

#[test]
fn sticky_address_is_parsed_from_pf_conf() {
    let ruleset =
        raven_pf::parse("nat on eth0 from 10.0.0.0/8 to any -> 203.0.113.9 sticky-address\n")
            .unwrap();
    assert_eq!(ruleset.nat.len(), 1);
    assert!(ruleset.nat.rules()[0].pool.sticky);
    assert_eq!(
        ruleset.nat.rules()[0].pool.addresses,
        vec![v4(203, 0, 113, 9)]
    );
}

#[test]
fn redirect_rewrites_the_destination_with_the_donor_formula() {
    let mut rdr = rule(
        NatAction::Rdr,
        PoolKind::None,
        vec![v4(10, 0, 0, 80)],
        Some((8080, 8090)),
        false,
    );
    rdr.destination.port = PortSpec::Range { start: 80, end: 90 };
    let mut table = NatTable::new();
    assert!(table.push(rdr));
    let inbound = Packet::tcp(
        Direction::In,
        "eth0",
        v4(10, 0, 0, 5),
        40000,
        v4(192, 0, 2, 10),
        85,
        tcp_flag::SYN,
        60,
    );
    assert_eq!(table.find(&inbound, &[]), Some(0));
    let translation = table.translate(0, &inbound, |_, _| false).unwrap().unwrap();
    assert_eq!(translation.source, v4(10, 0, 0, 5));
    assert_eq!(translation.source_port, 40000);
    assert_eq!(translation.destination, v4(10, 0, 0, 80));
    assert_eq!(translation.destination_port, 8085);
    assert_eq!(remap_redirect_port(80, 80, 8080, 8090), 8080);
    assert_eq!(remap_redirect_port(90, 80, 8080, 8090), 8090);
    assert_eq!(remap_redirect_port(91, 80, 8080, 8090), 8080);
}

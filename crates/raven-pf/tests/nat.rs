// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K298 integration: NAT/RDR IPv4 + IPv6, port ranges and sticky addresses.

use raven_pf::ast::{AddrPort, Direction, IfaceSpec, Packet, Protocol, tcp_flag};
use raven_pf::{
    AddrSpec, IpAddr, Ipv4Addr, Ipv6Addr, NatAction, NatError, NatRule, NatTable, Pool, PoolKind,
    parse,
};

fn addr4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(a, b, c, d))
}

fn addr6(text: &str) -> IpAddr {
    IpAddr::V6(Ipv6Addr::parse(text).expect("IPv6 literal"))
}

fn packet(
    source: IpAddr,
    source_port: u16,
    destination: IpAddr,
    destination_port: u16,
) -> Packet<'static> {
    Packet {
        direction: Direction::Out,
        interface: "eth0",
        protocol: Protocol::Tcp,
        source,
        destination,
        source_port,
        destination_port,
        length: 60,
        tcp_flags: tcp_flag::SYN,
        fragment: false,
    }
}

fn nat_rule(addresses: Vec<IpAddr>, kind: PoolKind, proxy_port: Option<(u16, u16)>) -> NatRule {
    NatRule {
        action: NatAction::Nat,
        direction: Some(Direction::Out),
        interface: IfaceSpec::Name("eth0".to_string()),
        protocol: Protocol::Any,
        source: AddrPort::any(),
        destination: AddrPort::any(),
        pool: Pool {
            kind,
            addresses,
            proxy_port,
            static_port: false,
        },
        log: false,
    }
}

#[test]
fn nat_translates_ipv6_source_and_keeps_the_destination() {
    let mut rs = parse("nat on eth0 from 2001:db8:1::1 to any -> 2001:db8:ffff::7\n")
        .expect("parse IPv6 NAT rule");
    assert_eq!(rs.nat.len(), 1);
    let outbound = packet(addr6("2001:db8:1::1"), 40000, addr6("2001:db8:2::80"), 443);
    assert_eq!(rs.nat.find(&outbound, &rs.tables), Some(0));
    let translation = rs
        .nat
        .translate(0, &outbound, |_, _| false)
        .expect("translate")
        .expect("matched rule");
    assert_eq!(translation.source, addr6("2001:db8:ffff::7"));
    assert_eq!(translation.destination, addr6("2001:db8:2::80"));
    assert_eq!(translation.destination_port, 443);
}

#[test]
fn nat_port_range_allocates_in_range_and_skips_live_ports() {
    let mut table = NatTable::new();
    assert!(table.push(nat_rule(
        vec![addr4(203, 0, 113, 7)],
        PoolKind::None,
        Some((50001, 50003)),
    )));
    let first = packet(addr4(10, 0, 0, 5), 1234, addr4(192, 0, 2, 10), 80);
    let translation = table
        .translate(0, &first, |_, _| false)
        .expect("translate")
        .expect("matched rule");
    assert_eq!(translation.source_port, 50001);

    let second = table
        .translate(0, &first, |_, port| port == 50001)
        .expect("translate")
        .expect("matched rule");
    assert_eq!(second.source_port, 50002);

    // Every port in the range is live: fail closed, never reuse or leave the range.
    assert_eq!(
        table.translate(0, &first, |_, _| true),
        Err(NatError::PoolExhausted)
    );
}

#[test]
fn a_sticky_nat_rule_keeps_the_same_address_for_a_source() {
    let mut table = NatTable::new();
    assert!(table.push(nat_rule(
        vec![addr4(203, 0, 113, 1), addr4(203, 0, 113, 2)],
        PoolKind::RoundRobin,
        Some((50001, 50010)),
    )));
    assert!(table.set_sticky(0, true));

    let client = packet(addr4(10, 0, 0, 5), 1000, addr4(192, 0, 2, 10), 80);
    let other = packet(addr4(10, 0, 0, 6), 1000, addr4(192, 0, 2, 10), 80);
    let first = table
        .translate(0, &client, |_, _| false)
        .expect("translate")
        .expect("matched rule");
    let again = table
        .translate(0, &client, |_, _| false)
        .expect("translate")
        .expect("matched rule");
    let different_source = table
        .translate(0, &other, |_, _| false)
        .expect("translate")
        .expect("matched rule");
    assert_eq!(first.source, addr4(203, 0, 113, 1));
    assert_eq!(again.source, first.source, "sticky source mapping");
    assert_eq!(different_source.source, addr4(203, 0, 113, 2));
}

#[test]
fn without_sticky_a_round_robin_pool_alternates_per_translation() {
    let mut table = NatTable::new();
    assert!(table.push(nat_rule(
        vec![addr4(203, 0, 113, 1), addr4(203, 0, 113, 2)],
        PoolKind::RoundRobin,
        Some((50001, 50010)),
    )));
    let client = packet(addr4(10, 0, 0, 5), 1000, addr4(192, 0, 2, 10), 80);
    let first = table
        .translate(0, &client, |_, _| false)
        .expect("translate")
        .expect("matched rule");
    let second = table
        .translate(0, &client, |_, _| false)
        .expect("translate")
        .expect("matched rule");
    assert_ne!(first.source, second.source);
}

#[test]
fn rdr_translates_ipv6_and_remaps_the_port_range() {
    let mut rdr = NatRule {
        action: NatAction::Rdr,
        direction: Some(Direction::In),
        interface: IfaceSpec::Name("eth0".to_string()),
        protocol: Protocol::Tcp,
        source: AddrPort::any(),
        destination: AddrPort {
            address: AddrSpec::parse("2001:db8:1::1").expect("address"),
            port: raven_pf::PortSpec::Range { start: 80, end: 90 },
        },
        pool: Pool {
            kind: PoolKind::None,
            addresses: vec![addr6("2001:db8:2::80")],
            proxy_port: Some((8080, 8090)),
            static_port: false,
        },
        log: false,
    };
    let mut table = NatTable::new();
    assert!(table.push(rdr.clone()));
    let mut inbound = packet(addr6("2001:db8:3::9"), 40000, addr6("2001:db8:1::1"), 85);
    inbound.direction = Direction::In;
    assert_eq!(table.find(&inbound, &[]), Some(0));
    let translation = table
        .translate(0, &inbound, |_, _| false)
        .expect("translate")
        .expect("matched rule");
    assert_eq!(translation.destination, addr6("2001:db8:2::80"));
    assert_eq!(translation.destination_port, 8085);
    rdr.pool.proxy_port = Some((8080, 8080));
    table = NatTable::new();
    assert!(table.push(rdr));
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for NAT and redirect translation.

use super::*;
use crate::address::{Ipv4Addr, Ipv6Addr, PortSpec};
use crate::ast::{Direction, Protocol, tcp_flag};

const CLIENT: Ipv4Addr = Ipv4Addr::new(10, 0, 0, 5);
const SERVER: Ipv4Addr = Ipv4Addr::new(192, 0, 2, 10);
const NATIP: Ipv4Addr = Ipv4Addr::new(203, 0, 113, 7);

fn out_tcp() -> Packet<'static> {
    Packet::tcp(
        Direction::Out,
        "eth0",
        CLIENT,
        1234,
        SERVER,
        80,
        tcp_flag::SYN,
        60,
    )
}

fn rule(action: NatAction, kind: PoolKind, addresses: Vec<IpAddr>) -> NatRule {
    NatRule {
        action,
        direction: Some(match action {
            NatAction::Nat => Direction::Out,
            NatAction::Rdr => Direction::In,
        }),
        interface: IfaceSpec::Name("eth0".to_string()),
        protocol: Protocol::Any,
        source: AddrPort::any(),
        destination: AddrPort::any(),
        pool: Pool {
            kind,
            addresses,
            proxy_port: Some((50001, 50003)),
            static_port: false,
        },
        log: false,
    }
}

#[test]
fn nat_rewrites_the_source_and_allocates_a_port() {
    let mut table = NatTable::new();
    assert!(table.push(rule(
        NatAction::Nat,
        PoolKind::None,
        alloc::vec![IpAddr::V4(NATIP)]
    )));
    assert_eq!(table.find(&out_tcp(), &[]), Some(0));
    let translation = table
        .translate(0, &out_tcp(), |_, _| false)
        .unwrap()
        .unwrap();
    assert_eq!(translation.source, IpAddr::V4(NATIP));
    assert_eq!(translation.source_port, 50001);
    assert_eq!(translation.destination, IpAddr::V4(SERVER));
    assert_eq!(translation.destination_port, 80);
}

#[test]
fn the_port_search_skips_ports_owned_by_live_states() {
    let mut table = NatTable::new();
    table.push(rule(
        NatAction::Nat,
        PoolKind::None,
        alloc::vec![IpAddr::V4(NATIP)],
    ));
    let translation = table
        .translate(0, &out_tcp(), |_, port| port == 50001)
        .unwrap()
        .unwrap();
    assert_eq!(translation.source_port, 50002);
}

#[test]
fn a_static_port_preserves_the_source_port() {
    let mut table = NatTable::new();
    let mut nat = rule(
        NatAction::Nat,
        PoolKind::None,
        alloc::vec![IpAddr::V4(NATIP)],
    );
    nat.pool.static_port = true;
    table.push(nat);
    let translation = table
        .translate(0, &out_tcp(), |_, _| false)
        .unwrap()
        .unwrap();
    assert_eq!(translation.source_port, 1234);
    assert_eq!(translation.source, IpAddr::V4(NATIP));
}

#[test]
fn an_exhausted_port_range_fails_closed() {
    let mut table = NatTable::new();
    let mut nat = rule(
        NatAction::Nat,
        PoolKind::None,
        alloc::vec![IpAddr::V4(NATIP)],
    );
    nat.pool.proxy_port = Some((50001, 50002));
    table.push(nat);
    let result = table.translate(0, &out_tcp(), |_, _| true);
    assert_eq!(result, Err(NatError::PoolExhausted));
}

#[test]
fn a_pool_without_an_address_fails_closed() {
    let mut table = NatTable::new();
    table.push(rule(NatAction::Nat, PoolKind::None, alloc::vec![]));
    assert_eq!(
        table.translate(0, &out_tcp(), |_, _| false),
        Err(NatError::NoAddress)
    );
}

#[test]
fn a_family_mismatch_fails_closed() {
    let mut table = NatTable::new();
    table.push(rule(
        NatAction::Nat,
        PoolKind::None,
        alloc::vec![IpAddr::V6(Ipv6Addr::parse("2001:db8::1").unwrap())],
    ));
    assert_eq!(
        table.translate(0, &out_tcp(), |_, _| false),
        Err(NatError::NoAddress)
    );
}

#[test]
fn redirect_rewrites_the_destination_and_port() {
    let mut table = NatTable::new();
    let mut rdr = rule(
        NatAction::Rdr,
        PoolKind::None,
        alloc::vec![IpAddr::V4(Ipv4Addr::new(10, 0, 0, 80))],
    );
    rdr.destination.port = PortSpec::Range { start: 80, end: 90 };
    rdr.pool.proxy_port = Some((8080, 8090));
    table.push(rdr);
    let inbound = Packet::tcp(
        Direction::In,
        "eth0",
        CLIENT,
        40000,
        NATIP,
        85,
        tcp_flag::SYN,
        60,
    );
    assert_eq!(table.find(&inbound, &[]), Some(0));
    let translation = table.translate(0, &inbound, |_, _| false).unwrap().unwrap();
    assert_eq!(translation.source, IpAddr::V4(CLIENT));
    assert_eq!(translation.source_port, 40000);
    assert_eq!(
        translation.destination,
        IpAddr::V4(Ipv4Addr::new(10, 0, 0, 80))
    );
    assert_eq!(translation.destination_port, 8085);
}

#[test]
fn the_donor_redirect_port_formula_is_reproduced() {
    assert_eq!(remap_redirect_port(80, 80, 8080, 8090), 8080);
    assert_eq!(remap_redirect_port(85, 80, 8080, 8090), 8085);
    assert_eq!(remap_redirect_port(90, 80, 8080, 8090), 8090);
    assert_eq!(remap_redirect_port(91, 80, 8080, 8090), 8080);
    // The modulo keeps the result inside the configured range; the donor's
    // `tmp_nport > 65535` subtraction is unreachable with a non-wrapping range.
    assert_eq!(remap_redirect_port(10, 1, 65530, 65535), 65533);
    // A fixed proxy port.
    assert_eq!(remap_redirect_port(80, 80, 8080, 8080), 8080);
    assert_eq!(remap_redirect_port(80, 80, 9000, 8000), 80);
}

#[test]
fn round_robin_pools_use_each_address_in_turn() {
    let mut table = NatTable::new();
    let addresses = alloc::vec![
        IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1)),
        IpAddr::V4(Ipv4Addr::new(203, 0, 113, 2)),
    ];
    table.push(rule(NatAction::Nat, PoolKind::RoundRobin, addresses));
    let first = table
        .translate(0, &out_tcp(), |_, _| false)
        .unwrap()
        .unwrap();
    let second = table
        .translate(0, &out_tcp(), |_, _| false)
        .unwrap()
        .unwrap();
    assert_eq!(first.source, IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1)));
    assert_eq!(second.source, IpAddr::V4(Ipv4Addr::new(203, 0, 113, 2)));
}

#[test]
fn source_hash_and_random_selection_are_deterministic() {
    let addresses = alloc::vec![
        IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1)),
        IpAddr::V4(Ipv4Addr::new(203, 0, 113, 2)),
    ];
    for kind in [PoolKind::SourceHash, PoolKind::Random] {
        let mut first = NatTable::new();
        first.push(rule(NatAction::Nat, kind, addresses.clone()));
        let mut second = NatTable::new();
        second.push(rule(NatAction::Nat, kind, addresses.clone()));
        let left = first.translate(0, &out_tcp(), |_, _| false).unwrap();
        let right = second.translate(0, &out_tcp(), |_, _| false).unwrap();
        assert_eq!(left, right, "{kind:?}");
    }
}

#[test]
fn the_rule_table_is_bounded() {
    let mut table = NatTable::new();
    for _ in 0..MAX_NAT_RULES {
        assert!(table.push(rule(
            NatAction::Nat,
            PoolKind::None,
            alloc::vec![IpAddr::V4(NATIP)]
        )));
    }
    assert!(!table.push(rule(
        NatAction::Nat,
        PoolKind::None,
        alloc::vec![IpAddr::V4(NATIP)]
    )));
}

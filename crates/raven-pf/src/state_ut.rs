// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the bounded state table and pf timeouts.

use super::*;
use crate::address::Ipv4Addr;
use crate::ast::tcp_flag;

const A: Ipv4Addr = Ipv4Addr::new(10, 0, 0, 1);
const B: Ipv4Addr = Ipv4Addr::new(192, 0, 2, 1);

fn syn() -> Packet<'static> {
    Packet::tcp(Direction::In, "eth0", A, 1234, B, 80, tcp_flag::SYN, 60)
}

#[test]
fn pf_default_timeouts_match_the_reference_table() {
    let timeouts = Timeouts::default();
    assert_eq!(timeouts.tcp_first, 120);
    assert_eq!(timeouts.tcp_opening, 30);
    assert_eq!(timeouts.tcp_established, 24 * 60 * 60);
    assert_eq!(timeouts.tcp_closing, 15 * 60);
    assert_eq!(timeouts.tcp_fin_wait, 45);
    assert_eq!(timeouts.tcp_closed, 90);
    assert_eq!(timeouts.udp_first, 60);
    assert_eq!(timeouts.udp_single, 30);
    assert_eq!(timeouts.udp_multiple, 60);
    assert_eq!(timeouts.icmp_first, 20);
    assert_eq!(timeouts.icmp_error, 10);
}

#[test]
fn tcp_handshake_moves_first_opening_established() {
    let mut table = StateTable::new(Timeouts::default());
    assert_eq!(
        table.establish(0, &syn(), Some("eth0"), 100),
        StateDisposition::Created
    );
    assert_eq!(table.entries()[0].expires, 220);

    let syn_ack = Packet::tcp(
        Direction::Out,
        "eth0",
        B,
        80,
        A,
        1234,
        tcp_flag::SYN | tcp_flag::ACK,
        60,
    );
    let hit = table.lookup(&syn_ack, 110).unwrap();
    assert_eq!(hit.orientation, Orientation::Reverse);
    assert_eq!(hit.tcp, TcpState::Opening);
    assert_eq!(table.entries()[0].expires, 140);

    let ack = Packet::tcp(Direction::In, "eth0", A, 1234, B, 80, tcp_flag::ACK, 40);
    let hit = table.lookup(&ack, 120).unwrap();
    assert_eq!(hit.tcp, TcpState::Established);
    assert_eq!(table.entries()[0].expires, 120 + 24 * 60 * 60);
}

#[test]
fn tcp_fin_and_rst_timeouts() {
    let mut table = StateTable::new(Timeouts::default());
    table.establish(0, &syn(), Some("eth0"), 0);
    let ack = Packet::tcp(Direction::In, "eth0", A, 1234, B, 80, tcp_flag::ACK, 40);
    table.lookup(&ack, 1).unwrap();
    let fin = Packet::tcp(Direction::In, "eth0", A, 1234, B, 80, tcp_flag::FIN, 40);
    let hit = table.lookup(&fin, 2).unwrap();
    assert_eq!(hit.tcp, TcpState::FinWait);
    assert_eq!(table.entries()[0].expires, 47);
    let fin_back = Packet::tcp(Direction::Out, "eth0", B, 80, A, 1234, tcp_flag::FIN, 40);
    let hit = table.lookup(&fin_back, 3).unwrap();
    assert_eq!(hit.tcp, TcpState::Closing);
    assert_eq!(table.entries()[0].expires, 3 + 15 * 60);

    let rst = Packet::tcp(Direction::In, "eth0", A, 1234, B, 80, tcp_flag::RST, 40);
    let hit = table.lookup(&rst, 4).unwrap();
    assert_eq!(hit.tcp, TcpState::Closed);
    assert_eq!(table.entries()[0].expires, 94);
}

#[test]
fn udp_timeouts_follow_direction_and_replies() {
    let mut table = StateTable::new(Timeouts::default());
    let first = Packet::udp(Direction::Out, "eth0", A, 5353, B, 53, 40);
    table.establish(0, &first, None, 10);
    assert_eq!(table.entries()[0].expires, 70);
    let second = Packet::udp(Direction::Out, "eth0", A, 5353, B, 53, 40);
    table.lookup(&second, 20).unwrap();
    assert_eq!(table.entries()[0].expires, 50);
    let reply = Packet::udp(Direction::In, "eth0", B, 53, A, 5353, 60);
    table.lookup(&reply, 30).unwrap();
    assert!(table.entries()[0].bidirectional);
    assert_eq!(table.entries()[0].expires, 90);
}

#[test]
fn icmp_replies_get_the_error_timeout() {
    let mut table = StateTable::new(Timeouts::default());
    let echo = Packet::icmp(Direction::Out, "eth0", A, B, 28);
    table.establish(0, &echo, None, 100);
    assert_eq!(table.entries()[0].expires, 120);
    let reply = Packet::icmp(Direction::In, "eth0", B, A, 28);
    let hit = table.lookup(&reply, 105).unwrap();
    assert_eq!(hit.orientation, Orientation::Reverse);
    assert_eq!(table.entries()[0].expires, 115);
}

#[test]
fn interface_binding_and_floating_states() {
    let mut bound = StateTable::new(Timeouts::default());
    bound.establish(0, &syn(), Some("eth0"), 0);
    let other = Packet::tcp(Direction::In, "eth1", A, 1234, B, 80, tcp_flag::SYN, 60);
    assert!(bound.lookup(&other, 1).is_none());

    let mut floating = StateTable::new(Timeouts::default());
    floating.establish(0, &syn(), None, 0);
    assert!(floating.lookup(&other, 1).is_some());
}

#[test]
fn orientation_checks_protocol_and_endpoints() {
    let key = StateKey::from_packet(&syn(), Some("eth0"));
    let wrong_proto = Packet::udp(Direction::In, "eth0", A, 1234, B, 80, 40);
    assert!(key.orientation(&wrong_proto).is_none());
    let wrong_port = Packet::tcp(Direction::In, "eth0", A, 1235, B, 80, tcp_flag::SYN, 40);
    assert!(key.orientation(&wrong_port).is_none());
    let same_direction_reverse =
        Packet::tcp(Direction::In, "eth0", B, 80, A, 1234, tcp_flag::ACK, 40);
    assert!(key.orientation(&same_direction_reverse).is_none());
}

#[test]
fn expire_removes_dead_entries_and_lookup_drops_one() {
    let mut table = StateTable::new(Timeouts::default());
    table.establish(0, &syn(), None, 0);
    assert_eq!(table.lookup(&syn(), 121), None);
    assert!(table.is_empty());
    table.establish(0, &syn(), None, 200);
    assert_eq!(table.expire(500), 1);
    assert!(table.is_empty());
}

#[test]
fn the_limit_evicts_the_earliest_expiry() {
    let mut table = StateTable::with_limit(2, Timeouts::default());
    let first = Packet::udp(Direction::Out, "eth0", A, 1, B, 53, 40);
    let second = Packet::udp(Direction::Out, "eth0", A, 2, B, 53, 40);
    let third = Packet::udp(Direction::Out, "eth0", A, 3, B, 53, 40);
    table.establish(0, &first, None, 0);
    table.establish(1, &second, None, 10);
    table.establish(2, &third, None, 20);
    assert_eq!(table.len(), 2);
    assert!(
        table
            .entries()
            .iter()
            .all(|entry| entry.key.source_port != 1)
    );
    assert_eq!(table.limit(), 2);
}

#[test]
fn establishing_an_existing_connection_refreshes_it() {
    let mut table = StateTable::new(Timeouts::default());
    assert_eq!(
        table.establish(0, &syn(), None, 0),
        StateDisposition::Created
    );
    assert_eq!(
        table.establish(0, &syn(), None, 10),
        StateDisposition::Existing
    );
    assert_eq!(table.len(), 1);
    assert_eq!(table.entries()[0].last_seen, 10);
}

#[test]
fn a_second_rule_cannot_take_over_an_existing_key() {
    let mut table = StateTable::new(Timeouts::default());
    table.establish(0, &syn(), None, 0);
    table.establish(7, &syn(), None, 10);
    assert_eq!(table.entries()[0].rule, 0);
}

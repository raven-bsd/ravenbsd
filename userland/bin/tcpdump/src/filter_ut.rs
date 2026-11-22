// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the clean-room filter parser and evaluator.

use super::*;

fn ethernet(ether_type: u16, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::new();
    frame.extend_from_slice(&[0x02, 0, 0, 0, 0, 0x0b]);
    frame.extend_from_slice(&[0x02, 0, 0, 0, 0, 0x0a]);
    frame.extend_from_slice(&ether_type.to_be_bytes());
    frame.extend_from_slice(payload);
    frame
}

fn ipv4(proto: u8, src: [u8; 4], dst: [u8; 4], frag: u16, payload: &[u8]) -> Vec<u8> {
    let total = 20 + payload.len();
    let mut header = Vec::new();
    header.push(0x45);
    header.push(0);
    header.extend_from_slice(&(total as u16).to_be_bytes());
    header.extend_from_slice(&0u16.to_be_bytes());
    header.extend_from_slice(&frag.to_be_bytes());
    header.push(64);
    header.push(proto);
    header.extend_from_slice(&0u16.to_be_bytes());
    header.extend_from_slice(&src);
    header.extend_from_slice(&dst);
    header.extend_from_slice(payload);
    header
}

fn tcp(sport: u16, dport: u16, flags: u8) -> Vec<u8> {
    let mut header = Vec::new();
    header.extend_from_slice(&sport.to_be_bytes());
    header.extend_from_slice(&dport.to_be_bytes());
    header.extend_from_slice(&1000u32.to_be_bytes());
    header.extend_from_slice(&0u32.to_be_bytes());
    header.push(5 << 4);
    header.push(flags);
    header.extend_from_slice(&65535u16.to_be_bytes());
    header.extend_from_slice(&0u16.to_be_bytes());
    header.extend_from_slice(&0u16.to_be_bytes());
    header
}

fn udp(sport: u16, dport: u16) -> Vec<u8> {
    let mut header = Vec::new();
    header.extend_from_slice(&sport.to_be_bytes());
    header.extend_from_slice(&dport.to_be_bytes());
    header.extend_from_slice(&8u16.to_be_bytes());
    header.extend_from_slice(&0u16.to_be_bytes());
    header
}

fn icmp(kind: u8) -> Vec<u8> {
    vec![kind, 0, 0, 0, 0x12, 0x34, 0, 1]
}

fn ipv4_tcp(flags: u8) -> Vec<u8> {
    ethernet(
        0x0800,
        &ipv4(6, [10, 0, 0, 5], [192, 0, 2, 10], 0, &tcp(40000, 80, flags)),
    )
}

fn ipv4_udp(sport: u16, dport: u16) -> Vec<u8> {
    ethernet(
        0x0800,
        &ipv4(17, [10, 0, 0, 5], [192, 0, 2, 53], 0, &udp(sport, dport)),
    )
}

fn ipv4_icmp() -> Vec<u8> {
    ethernet(
        0x0800,
        &ipv4(1, [10, 0, 0, 5], [192, 0, 2, 10], 0, &icmp(8)),
    )
}

fn ipv4_fragment(frag: u16) -> Vec<u8> {
    ethernet(
        0x0800,
        &ipv4(17, [10, 0, 0, 5], [192, 0, 2, 10], frag, &[0u8; 8]),
    )
}

fn ipv6_tcp() -> Vec<u8> {
    let mut header = Vec::new();
    header.extend_from_slice(&0x6000_0000u32.to_be_bytes());
    header.extend_from_slice(&20u16.to_be_bytes());
    header.push(6);
    header.push(64);
    header.extend_from_slice(&[0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5]);
    header.extend_from_slice(&[0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 10]);
    header.extend_from_slice(&tcp(40000, 80, 0x02));
    ethernet(0x86dd, &header)
}

fn matches(expression: &str, frame: &[u8]) -> bool {
    compile(expression)
        .unwrap_or_else(|error| panic!("{expression}: {error}"))
        .matches(frame)
}

#[test]
fn protocol_primitives() {
    assert!(matches("tcp", &ipv4_tcp(0x02)));
    assert!(!matches("udp", &ipv4_tcp(0x02)));
    assert!(matches("udp", &ipv4_udp(53000, 53)));
    assert!(matches("icmp", &ipv4_icmp()));
    assert!(matches("ip", &ipv4_tcp(0x02)));
    assert!(!matches("ip6", &ipv4_tcp(0x02)));
    assert!(matches("ip6 and tcp", &ipv6_tcp()));
}

#[test]
fn tcp_port_and_host_qualifiers() {
    assert!(matches("tcp port 80", &ipv4_tcp(0x02)));
    assert!(!matches("tcp port 81", &ipv4_tcp(0x02)));
    assert!(matches("src host 10.0.0.5", &ipv4_tcp(0x02)));
    assert!(!matches("dst host 10.0.0.5", &ipv4_tcp(0x02)));
    assert!(matches("host 192.0.2.10", &ipv4_tcp(0x02)));
    assert!(matches("udp port 53", &ipv4_udp(53000, 53)));
}

#[test]
fn tcp_flag_byte_expression() {
    assert!(matches("tcp[tcpflags] & tcp-syn != 0", &ipv4_tcp(0x02)));
    assert!(!matches("tcp[tcpflags] & tcp-syn != 0", &ipv4_tcp(0x10)));
}

#[test]
fn icmp_type_byte_expression() {
    assert!(matches("icmp[icmptype] == icmp-echo", &ipv4_icmp()));
}

#[test]
fn ip_fragment_byte_expressions() {
    assert!(matches("ip[6:2] & 0x1fff != 0", &ipv4_fragment(0x0001)));
    assert!(!matches("ip[6:2] & 0x1fff != 0", &ipv4_fragment(0x2000)));
    assert!(matches("ip[6:2] & 0x2000 != 0", &ipv4_fragment(0x2000)));
    assert!(!matches("ip[6:2] & 0x2000 != 0", &ipv4_fragment(0x0001)));
}

#[test]
fn boolean_composition() {
    assert!(matches("not udp", &ipv4_tcp(0x02)));
    assert!(!matches("not tcp", &ipv4_tcp(0x02)));
    assert!(matches("(tcp or udp) and port 53", &ipv4_udp(53000, 53)));
    assert!(!matches("(tcp or udp) and port 53", &ipv4_tcp(0x02)));
}

#[test]
fn rejects_a_filter_that_must_not_match() {
    assert!(!matches("tcp port 9999", &ipv4_tcp(0x02)));
}

#[test]
fn rejects_a_malformed_filter() {
    assert!(compile("").is_err());
    assert!(compile("bogus").is_err());
    assert!(compile("tcp port not-a-number").is_err());
    assert!(compile("tcp[tcpflags] & 0x02 ==").is_err());
}

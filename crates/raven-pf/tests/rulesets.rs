// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Integration tests over the B17 fixture rulesets and packets.

use raven_pf::{
    Action, AddrSpec, Direction, Engine, IfaceSpec, IpAddr, Ipv4Addr, Packet, PortSpec, Protocol,
    StatePolicy, Verdict, parse,
};

const BASIC: &str = include_str!("../../../tests/fixtures/b17-pf/basic.conf");
const STATEFUL: &str = include_str!("../../../tests/fixtures/b17-pf/stateful.conf");
const INVALID: &str = include_str!("../../../tests/fixtures/b17-pf/invalid.conf");
const PACKETS: &str = include_str!("../../../tests/fixtures/b17-pf/packets.tsv");

#[test]
fn the_basic_fixture_parses_its_five_rules() {
    let ruleset = parse(BASIC).unwrap();
    assert_eq!(ruleset.default_action, Action::Block);
    assert_eq!(ruleset.rules.len(), 4);

    let loopback = &ruleset.rules[0];
    assert_eq!(loopback.action, Action::Pass);
    assert!(loopback.quick);
    assert_eq!(loopback.direction, Some(Direction::In));
    assert_eq!(loopback.interface, IfaceSpec::Name("lo".into()));

    let udp = &ruleset.rules[1];
    assert_eq!(udp.action, Action::Block);
    assert_eq!(udp.protocol, Protocol::Udp);

    let https = &ruleset.rules[2];
    assert_eq!(https.source.address, AddrSpec::parse("10.0.0.0/8").unwrap());
    assert_eq!(
        https.destination.address,
        AddrSpec::parse("192.0.2.0/24").unwrap()
    );
    assert_eq!(https.destination.port, PortSpec::Equal(443));
    assert_eq!(https.state, StatePolicy::Keep);

    let outbound = &ruleset.rules[3];
    assert_eq!(outbound.action, Action::Pass);
    assert_eq!(outbound.direction, Some(Direction::Out));
    assert_eq!(outbound.state, StatePolicy::None);
}

#[test]
fn the_invalid_fixture_names_line_eight() {
    let error = parse(INVALID).unwrap_err();
    assert_eq!(error.line, 8);
    assert!(error.message.contains("unknown protocol"), "{error}");
}

#[test]
fn the_stateful_packet_fixture_matches() {
    let ruleset = parse(STATEFUL).unwrap();
    let mut engine = Engine::new(ruleset);
    let mut now = 0u64;
    for (index, line) in PACKETS.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(fields.len(), 10, "line {}: {line}", index + 1);
        let direction = match fields[0] {
            "in" => Direction::In,
            "out" => Direction::Out,
            other => panic!("direction {other}"),
        };
        let interface = fields[1];
        let protocol = match fields[2] {
            "tcp" => Protocol::Tcp,
            "udp" => Protocol::Udp,
            "icmp" => Protocol::Icmp,
            other => panic!("protocol {other}"),
        };
        let source = IpAddr::V4(Ipv4Addr::parse(fields[3]).unwrap());
        let source_port = fields[4].parse().unwrap();
        let destination = IpAddr::V4(Ipv4Addr::parse(fields[5]).unwrap());
        let destination_port = fields[6].parse().unwrap();
        let length = fields[7].parse().unwrap();
        let flags = parse_flags(fields[8]);
        let expected = match fields[9] {
            "pass" => Verdict::Pass,
            "block" => Verdict::Block,
            other => panic!("verdict {other}"),
        };
        let packet = Packet {
            direction,
            interface,
            protocol,
            source,
            destination,
            source_port,
            destination_port,
            length,
            tcp_flags: flags,
            fragment: false,
        };
        let decision = engine.evaluate(&packet, now);
        assert_eq!(decision.verdict, expected, "line {}: {line}", index + 1);
        now += 1;
    }
    let counters = engine.counters();
    assert_eq!(counters.in_pass, 2);
    assert_eq!(counters.out_pass, 1);
    assert_eq!(counters.in_block, 2);
    assert_eq!(counters.states_created, 1);
    assert_eq!(counters.state_hits, 2);
}

fn parse_flags(text: &str) -> u8 {
    if text == "-" {
        return 0;
    }
    let mut flags = 0u8;
    for byte in text.bytes() {
        flags |= match byte {
            b'S' => raven_pf::ast::tcp_flag::SYN,
            b'A' => raven_pf::ast::tcp_flag::ACK,
            b'F' => raven_pf::ast::tcp_flag::FIN,
            b'R' => raven_pf::ast::tcp_flag::RST,
            other => panic!("flag {}", char::from(other)),
        };
    }
    flags
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Integration tests over the B17 fixture rulesets and packets.

use raven_pf::{
    Action, AddrSpec, Direction, Engine, IfaceSpec, Ipv4Addr, Ipv6Addr, Packet, Packet6, PortSpec,
    Protocol, StatePolicy, Verdict, parse,
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
        let source = Ipv4Addr::parse(fields[3]).unwrap();
        let source_port = fields[4].parse().unwrap();
        let destination = Ipv4Addr::parse(fields[5]).unwrap();
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

#[test]
fn tables_expand_into_rules() {
    let ruleset = parse("table <lan> { 10.0.0.0/8, 192.0.2.1 }\npass in from <lan> to any\n")
        .expect("table ruleset");
    assert_eq!(ruleset.tables.len(), 1);
    assert_eq!(ruleset.tables[0].addresses, vec!["10.0.0.0/8", "192.0.2.1"]);
    assert_eq!(ruleset.rules.len(), 2);

    let mut engine = Engine::new(ruleset);
    let inside = Packet::tcp(
        Direction::In,
        "eth0",
        Ipv4Addr::new(192, 0, 2, 1),
        1234,
        Ipv4Addr::new(203, 0, 113, 9),
        80,
        0,
        64,
    );
    assert_eq!(engine.evaluate(&inside, 0).verdict, Verdict::Pass);
    let outside = Packet::tcp(
        Direction::In,
        "eth0",
        Ipv4Addr::new(203, 0, 113, 9),
        1234,
        Ipv4Addr::new(192, 0, 2, 1),
        80,
        0,
        64,
    );
    assert_eq!(engine.evaluate(&outside, 0).verdict, Verdict::Block);
}

#[test]
fn nat_and_redirect_translate_the_expected_endpoint() {
    let ruleset = parse(
        "nat on eth0 from any to any -> 203.0.113.5\n\
         rdr on eth0 proto tcp from any to any port 80 -> 127.0.0.1 port 8080\n",
    )
    .expect("nat ruleset");
    let engine = Engine::new(ruleset);
    let outbound = Packet::tcp(
        Direction::Out,
        "eth0",
        Ipv4Addr::new(10, 0, 2, 15),
        4000,
        Ipv4Addr::new(198, 51, 100, 1),
        80,
        0,
        64,
    );
    let translation = engine.translate(&outbound).expect("nat");
    assert!(!translation.destination);
    assert_eq!(
        translation.address,
        AddrSpec::Host(Ipv4Addr::new(203, 0, 113, 5))
    );
    assert_eq!(translation.port, 4000);

    let inbound = Packet::tcp(
        Direction::In,
        "eth0",
        Ipv4Addr::new(198, 51, 100, 1),
        5000,
        Ipv4Addr::new(203, 0, 113, 5),
        80,
        0,
        64,
    );
    let translation = engine.translate(&inbound).expect("rdr");
    assert!(translation.destination);
    assert_eq!(
        translation.address,
        AddrSpec::Host(Ipv4Addr::new(127, 0, 0, 1))
    );
    assert_eq!(translation.port, 8080);
}

#[test]
fn ipv6_static_rules_match_and_default_block() {
    let source = Ipv6Addr::parse("2001:db8::1").expect("v6 source");
    let destination = Ipv6Addr::parse("2001:db8:1::2").expect("v6 destination");
    assert!(
        AddrSpec::parse("2001:db8::/32")
            .expect("v6 cidr")
            .matches_v6(source)
    );
    let ruleset = parse("pass in proto tcp from 2001:db8::/32 to any\n").expect("v6 rule");
    let mut engine = Engine::new(ruleset);
    let packet = Packet6 {
        direction: Direction::In,
        interface: "eth0",
        protocol: Protocol::Tcp,
        source,
        destination,
        source_port: 1234,
        destination_port: 443,
        length: 64,
        tcp_flags: 0,
    };
    assert_eq!(engine.evaluate_v6(&packet).verdict, Verdict::Pass);
    let outside = Packet6 {
        source: Ipv6Addr::parse("2001:db9::1").unwrap(),
        ..packet
    };
    assert_eq!(engine.evaluate_v6(&outside).verdict, Verdict::Block);
}

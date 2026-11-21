// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K273 acceptance tests over the pf fixture rulesets and packet tables.
//!
//! The packet rows cover IPv4 and IPv6 allow/deny, the state shortcut in both
//! directions, NAT, redirect, fragmentation and the fail-closed parser. Donor
//! traceability is in the report Evidence block.

use raven_pf::ast::tcp_flag;
use raven_pf::{
    Action, AddrSpec, Direction, Engine, IpAddr, Packet, Protocol, StateDisposition, Verdict, parse,
};

const CORE: &str = include_str!("../../../tests/fixtures/pf/core.conf");
const CORE_PACKETS: &str = include_str!("../../../tests/fixtures/pf/core.tsv");
const INVALID: &str = include_str!("../../../tests/fixtures/pf/invalid.conf");

fn address(text: &str) -> IpAddr {
    IpAddr::parse(text).unwrap()
}

#[test]
fn the_core_fixture_parses_rules_tables_and_translations() {
    let ruleset = parse(CORE).unwrap();
    assert_eq!(ruleset.default_action, Action::Block);
    assert_eq!(ruleset.rules.len(), 7);
    assert_eq!(ruleset.tables.len(), 1);
    assert_eq!(ruleset.tables[0].name, "web");
    assert_eq!(ruleset.tables[0].entries.len(), 3);
    assert_eq!(ruleset.nat.len(), 2);

    let logged = &ruleset.rules[2];
    assert_eq!(
        logged.source.address,
        AddrSpec::parse("10.0.0.0/8").unwrap()
    );
    assert_eq!(logged.destination.address, AddrSpec::Table(0));
    assert!(logged.log);
    assert!(matches!(ruleset.rules[1].action, Action::Block));
}

#[test]
fn the_invalid_fixture_is_rejected_on_its_bad_rule_line() {
    let error = parse(INVALID).unwrap_err();
    assert_eq!(error.line, 10);
    assert!(error.message.contains("unknown protocol"), "{error}");
}

#[test]
fn the_core_packet_fixture_matches() {
    let ruleset = parse(CORE).unwrap();
    let mut engine = Engine::new(ruleset);
    let mut now = 0u64;
    for (index, line) in CORE_PACKETS.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(fields.len(), 11, "line {}: {line}", index + 1);
        let direction = match fields[0] {
            "in" => Direction::In,
            "out" => Direction::Out,
            other => panic!("direction {other}"),
        };
        let protocol = match fields[2] {
            "tcp" => Protocol::Tcp,
            "udp" => Protocol::Udp,
            "icmp" => Protocol::Icmp,
            "icmp6" => Protocol::Icmpv6,
            other => panic!("protocol {other}"),
        };
        let packet = Packet {
            direction,
            interface: fields[1],
            protocol,
            source: address(fields[3]),
            source_port: fields[4].parse().unwrap(),
            destination: address(fields[5]),
            destination_port: fields[6].parse().unwrap(),
            length: fields[7].parse().unwrap(),
            tcp_flags: parse_flags(fields[8]),
            fragment: fields[9] == "frag",
        };
        let expected = match fields[10] {
            "pass" => Verdict::Pass,
            "block" => Verdict::Block,
            other => panic!("verdict {other}"),
        };
        let decision = engine.evaluate(&packet, now);
        assert_eq!(decision.verdict, expected, "line {}: {line}", index + 1);
        now += 1;
    }
    let counters = engine.counters();
    assert_eq!(counters.in_pass, 4);
    assert_eq!(counters.in_block, 5);
    assert_eq!(counters.out_pass, 3);
    assert_eq!(counters.out_block, 0);
    assert_eq!(counters.states_created, 5);
    assert_eq!(counters.state_hits, 2);
    // Only the `log` rule on the matching pass path logs; a fragment never matches a
    // rule that names a port (donor PF_VPROTO_FRAGMENT), so row 10 blocks without a log.
    assert_eq!(engine.logs().len(), 1);
}

#[test]
fn nat_translates_an_outbound_flow_and_its_reply() {
    let mut engine = Engine::new(parse(CORE).unwrap());
    let outbound = Packet::tcp(
        Direction::Out,
        "eth0",
        address("10.0.0.5"),
        40000,
        address("8.8.8.8"),
        53,
        tcp_flag::SYN,
        60,
    );
    let decision = engine.evaluate(&outbound, 0);
    assert_eq!(decision.verdict, Verdict::Pass);
    assert_eq!(decision.state, Some(StateDisposition::Created));
    let translation = decision.translation.expect("NAT translation");
    assert_eq!(translation.source, address("203.0.113.7"));
    assert_eq!(translation.source_port, raven_pf::nat::NAT_PROXY_PORT_LOW);
    assert_eq!(translation.destination, address("8.8.8.8"));
    assert_eq!(translation.destination_port, 53);

    let reply = Packet::tcp(
        Direction::In,
        "eth0",
        address("8.8.8.8"),
        53,
        address("203.0.113.7"),
        translation.source_port,
        tcp_flag::SYN | tcp_flag::ACK,
        60,
    );
    let decision = engine.evaluate(&reply, 1);
    assert_eq!(decision.verdict, Verdict::Pass);
    assert_eq!(decision.state, Some(StateDisposition::Existing));
    let back = decision.translation.expect("reverse NAT translation");
    assert_eq!(back.source, address("8.8.8.8"));
    assert_eq!(back.destination, address("10.0.0.5"));
    assert_eq!(back.destination_port, 40000);
}

#[test]
fn redirect_rewrites_the_destination_and_its_reply() {
    let mut engine = Engine::new(parse(CORE).unwrap());
    let inbound = Packet::tcp(
        Direction::In,
        "eth0",
        address("198.51.100.9"),
        40000,
        address("203.0.113.7"),
        80,
        tcp_flag::SYN,
        60,
    );
    let decision = engine.evaluate(&inbound, 0);
    assert_eq!(decision.verdict, Verdict::Pass);
    let translation = decision.translation.expect("RDR translation");
    assert_eq!(translation.destination, address("10.0.0.80"));
    assert_eq!(translation.destination_port, 8080);

    let reply = Packet::tcp(
        Direction::Out,
        "eth0",
        address("10.0.0.80"),
        8080,
        address("198.51.100.9"),
        40000,
        tcp_flag::SYN | tcp_flag::ACK,
        60,
    );
    let decision = engine.evaluate(&reply, 1);
    assert_eq!(decision.verdict, Verdict::Pass);
    let back = decision.translation.expect("reverse RDR translation");
    assert_eq!(back.source, address("203.0.113.7"));
    assert_eq!(back.source_port, 80);
    assert_eq!(back.destination, address("198.51.100.9"));
}

fn parse_flags(text: &str) -> u8 {
    if text == "-" {
        return 0;
    }
    let mut flags = 0u8;
    for byte in text.bytes() {
        flags |= match byte {
            b'S' => tcp_flag::SYN,
            b'A' => tcp_flag::ACK,
            b'F' => tcp_flag::FIN,
            b'R' => tcp_flag::RST,
            other => panic!("flag {}", char::from(other)),
        };
    }
    flags
}

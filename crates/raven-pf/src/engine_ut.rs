// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the match engine, counters and stateful shortcut.

use super::*;
use crate::address::{IpAddr, Ipv4Addr, Ipv6Addr};
use crate::ast::{Direction, tcp_flag};
use crate::parser::parse;

const CLIENT: Ipv4Addr = Ipv4Addr::new(10, 0, 0, 5);
const SERVER: Ipv4Addr = Ipv4Addr::new(192, 0, 2, 10);
const NATIP: Ipv4Addr = Ipv4Addr::new(203, 0, 113, 7);

fn inbound() -> Packet<'static> {
    Packet::tcp(
        Direction::In,
        "eth0",
        CLIENT,
        40000,
        SERVER,
        80,
        tcp_flag::SYN,
        60,
    )
}

fn reply() -> Packet<'static> {
    Packet::tcp(
        Direction::Out,
        "eth0",
        SERVER,
        80,
        CLIENT,
        40000,
        tcp_flag::SYN | tcp_flag::ACK,
        60,
    )
}

#[test]
fn an_empty_ruleset_blocks_by_default_and_counts_it() {
    let mut engine = Engine::new(Ruleset::new());
    let decision = engine.evaluate(&inbound(), 0);
    assert_eq!(decision.verdict, Verdict::Block);
    assert_eq!(decision.rule, None);
    assert_eq!(decision.reason, Some(crate::abi::PFRES_MATCH));
    assert_eq!(engine.counters().in_block, 1);
    assert_eq!(engine.counters().reason(crate::abi::PFRES_MATCH), 1);
}

#[test]
fn a_pass_rule_creates_state_and_allows_the_reply() {
    let ruleset =
        parse("pass in on eth0 proto tcp from 10.0.0.0/24 to any port 80 keep state\n").unwrap();
    let mut engine = Engine::new(ruleset);
    let decision = engine.evaluate(&inbound(), 10);
    assert_eq!(decision.verdict, Verdict::Pass);
    assert_eq!(decision.rule, Some(0));
    assert_eq!(decision.state, Some(StateDisposition::Created));
    assert_eq!(engine.states().len(), 1);
    let decision = engine.evaluate(&reply(), 20);
    assert_eq!(decision.verdict, Verdict::Pass);
    assert_eq!(decision.state, Some(StateDisposition::Existing));
    let counters = engine.counters();
    assert_eq!(counters.in_pass, 1);
    assert_eq!(counters.out_pass, 1);
    assert_eq!(counters.states_created, 1);
    assert_eq!(counters.state_hits, 1);
    let stats = engine.rule_stats(0).unwrap();
    assert_eq!(stats.evaluations, 1);
    assert_eq!(stats.matches, 1);
    assert_eq!(stats.packets, 2);
    assert_eq!(stats.bytes, 120);
}

#[test]
fn state_is_checked_before_the_rules() {
    let ruleset = parse(
        "pass in on eth0 proto tcp from any to any port 80 keep state\n\
         block quick out all\n",
    )
    .unwrap();
    let mut engine = Engine::new(ruleset);
    assert_eq!(engine.evaluate(&inbound(), 0).verdict, Verdict::Pass);
    assert_eq!(engine.evaluate(&reply(), 1).verdict, Verdict::Pass);
    assert_eq!(engine.counters().state_hits, 1);
    assert_eq!(engine.rule_stats(1).unwrap().packets, 0);
}

#[test]
fn no_state_rules_re_evaluate_every_packet() {
    let ruleset = parse("pass in on eth0 all no state\n").unwrap();
    let mut engine = Engine::new(ruleset);
    assert_eq!(engine.evaluate(&inbound(), 0).verdict, Verdict::Pass);
    assert_eq!(engine.evaluate(&inbound(), 1).verdict, Verdict::Pass);
    assert!(engine.states().is_empty());
    assert_eq!(engine.counters().in_pass, 2);
    assert_eq!(engine.rule_stats(0).unwrap().evaluations, 2);
    assert_eq!(engine.rule_stats(0).unwrap().matches, 2);
}

#[test]
fn the_last_match_wins_unless_a_rule_is_quick() {
    let ruleset = parse("block in all\npass in all\n").unwrap();
    let mut engine = Engine::new(ruleset);
    assert_eq!(engine.evaluate(&inbound(), 0).verdict, Verdict::Pass);
    assert_eq!(engine.rule_stats(0).unwrap().matches, 1);
    assert_eq!(engine.rule_stats(1).unwrap().matches, 1);

    let ruleset = parse("block quick in all\npass in all\n").unwrap();
    let mut engine = Engine::new(ruleset);
    assert_eq!(engine.evaluate(&inbound(), 0).verdict, Verdict::Block);
    assert_eq!(engine.rule_stats(1).unwrap().evaluations, 0);
}

#[test]
fn default_action_can_pass() {
    let ruleset = parse("set default pass\nblock in proto udp from any to any\n").unwrap();
    let mut engine = Engine::new(ruleset);
    assert_eq!(engine.evaluate(&inbound(), 0).verdict, Verdict::Pass);
    assert_eq!(engine.evaluate(&inbound(), 0).rule, None);
}

#[test]
fn interface_protocol_address_and_port_all_filter() {
    let ruleset = parse(
        "pass in on lo all\n\
         pass in on eth0 proto udp from any to any\n\
         pass in on eth0 proto tcp from 198.51.100.0/24 to any\n\
         pass in on eth0 proto tcp from any to any port 22\n",
    )
    .unwrap();
    let mut engine = Engine::new(ruleset);
    assert_eq!(engine.evaluate(&inbound(), 0).verdict, Verdict::Block);
    let port22 = Packet::tcp(
        Direction::In,
        "eth0",
        CLIENT,
        50000,
        SERVER,
        22,
        tcp_flag::SYN,
        60,
    );
    assert_eq!(engine.evaluate(&port22, 1).verdict, Verdict::Pass);
    let udp = Packet::udp(Direction::In, "eth0", CLIENT, 1, SERVER, 2, 40);
    assert_eq!(engine.evaluate(&udp, 2).verdict, Verdict::Pass);
    let loopback = Packet::tcp(Direction::In, "lo", CLIENT, 1, SERVER, 2, 0, 40);
    assert_eq!(engine.evaluate(&loopback, 3).verdict, Verdict::Pass);
}

#[test]
fn an_expired_state_falls_back_to_the_rules() {
    let ruleset = parse("pass in on eth0 proto tcp from any to any keep state\n").unwrap();
    let mut engine = Engine::new(ruleset);
    assert_eq!(engine.evaluate(&inbound(), 0).verdict, Verdict::Pass);
    assert_eq!(engine.evaluate(&reply(), 121).verdict, Verdict::Block);
    assert!(engine.states().is_empty());
    assert_eq!(engine.counters().states_expired, 1);
}

#[test]
fn the_state_limit_fails_closed_and_is_counted() {
    // Donor pf_create_state(): max_states check -> PFRES_MAXSTATES -> PF_DROP.
    let ruleset = parse("pass in on eth0 proto udp from any to any keep state\n").unwrap();
    let mut engine = Engine::with_limit(ruleset, Timeouts::default(), 1);
    let first = Packet::udp(Direction::In, "eth0", CLIENT, 1, SERVER, 53, 40);
    let second = Packet::udp(Direction::In, "eth0", CLIENT, 2, SERVER, 53, 40);
    assert_eq!(engine.evaluate(&first, 0).verdict, Verdict::Pass);
    let decision = engine.evaluate(&second, 1);
    assert_eq!(decision.verdict, Verdict::Block);
    assert_eq!(decision.state, Some(StateDisposition::Rejected));
    assert_eq!(decision.reason, Some(crate::abi::PFRES_MAXSTATES));
    assert_eq!(engine.states().len(), 1);
    assert_eq!(engine.counters().reason(crate::abi::PFRES_MAXSTATES), 1);
    // The fail-closed drop is logged even though the rule has no `log`.
    let logs = engine.logs();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].reason, Some(crate::abi::PFRES_MAXSTATES));
}

#[test]
fn rule_stats_are_bounded() {
    let engine = Engine::new(Ruleset::new());
    assert!(engine.rule_stats(0).is_none());
    assert_eq!(engine.ruleset().rules.len(), 0);
}

#[test]
fn blocking_counts_bytes_on_the_matching_rule() {
    let ruleset = parse("block in proto udp from any to any\n").unwrap();
    let mut engine = Engine::new(ruleset);
    let packet = Packet::udp(Direction::In, "eth0", CLIENT, 1, SERVER, 2, 100);
    assert_eq!(engine.evaluate(&packet, 0).verdict, Verdict::Block);
    let stats = engine.rule_stats(0).unwrap();
    assert_eq!(stats.matches, 1);
    assert_eq!(stats.packets, 1);
    assert_eq!(stats.bytes, 100);
    assert_eq!(engine.counters().in_block, 1);
}

#[test]
fn ipv6_packets_are_filtered_and_stateful() {
    let ruleset = parse(
        "pass in inet6 proto tcp from 2001:db8::/32 to 2001:db8:1::/48 port 443 keep state\n",
    )
    .unwrap();
    let mut engine = Engine::new(ruleset);
    let source = Ipv6Addr::parse("2001:db8::5").unwrap();
    let server = Ipv6Addr::parse("2001:db8:1::10").unwrap();
    let syn = Packet::tcp(
        Direction::In,
        "eth0",
        source,
        40000,
        server,
        443,
        tcp_flag::SYN,
        60,
    );
    assert_eq!(engine.evaluate(&syn, 0).verdict, Verdict::Pass);
    assert_eq!(engine.states().len(), 1);
    let reply = Packet::tcp(
        Direction::Out,
        "eth0",
        server,
        443,
        source,
        40000,
        tcp_flag::SYN | tcp_flag::ACK,
        60,
    );
    assert_eq!(engine.evaluate(&reply, 1).verdict, Verdict::Pass);
    assert_eq!(engine.counters().state_hits, 1);

    let outside = Packet::tcp(
        Direction::In,
        "eth0",
        Ipv6Addr::parse("2001:db9::5").unwrap(),
        40000,
        server,
        443,
        tcp_flag::SYN,
        60,
    );
    assert_eq!(engine.evaluate(&outside, 2).verdict, Verdict::Block);
}

#[test]
fn a_table_reference_filters_by_membership() {
    let ruleset = parse(
        "table <web> { 192.0.2.10, 192.0.2.11 }\n\
         pass in proto tcp from any to <web> port 80 keep state\n",
    )
    .unwrap();
    assert_eq!(ruleset.tables.len(), 1);
    let mut engine = Engine::new(ruleset);
    assert_eq!(engine.evaluate(&inbound(), 0).verdict, Verdict::Pass);
    let outsider = Packet::tcp(
        Direction::In,
        "eth0",
        CLIENT,
        40001,
        Ipv4Addr::new(192, 0, 2, 12),
        80,
        tcp_flag::SYN,
        60,
    );
    assert_eq!(engine.evaluate(&outsider, 1).verdict, Verdict::Block);
}

#[test]
fn nat_translates_the_flow_and_its_reply() {
    let ruleset = parse(
        "pass out on eth0 all keep state\n\
         nat on eth0 from any to any -> 203.0.113.7\n",
    )
    .unwrap();
    let mut engine = Engine::new(ruleset);
    let outbound = Packet::tcp(
        Direction::Out,
        "eth0",
        CLIENT,
        1234,
        SERVER,
        80,
        tcp_flag::SYN,
        60,
    );
    let decision = engine.evaluate(&outbound, 0);
    assert_eq!(decision.verdict, Verdict::Pass);
    let translation = decision.translation.expect("NAT translation");
    assert_eq!(translation.source, IpAddr::V4(NATIP));
    assert_eq!(translation.source_port, crate::nat::NAT_PROXY_PORT_LOW);
    assert_eq!(translation.destination, IpAddr::V4(SERVER));

    let reply = Packet::tcp(
        Direction::In,
        "eth0",
        SERVER,
        80,
        NATIP,
        translation.source_port,
        tcp_flag::SYN | tcp_flag::ACK,
        60,
    );
    let decision = engine.evaluate(&reply, 1);
    assert_eq!(decision.verdict, Verdict::Pass);
    assert_eq!(decision.state, Some(StateDisposition::Existing));
    let back = decision.translation.expect("reverse translation");
    assert_eq!(back.source, IpAddr::V4(SERVER));
    assert_eq!(back.destination, IpAddr::V4(CLIENT));
    assert_eq!(back.destination_port, 1234);
}

#[test]
fn redirect_rewrites_an_inbound_flow() {
    let ruleset = parse(
        "pass in on eth0 proto tcp from any to any keep state\n\
         rdr on eth0 proto tcp from any to 203.0.113.7 port 80 -> 10.0.0.80 port 8080\n",
    )
    .unwrap();
    let mut engine = Engine::new(ruleset);
    let inbound = Packet::tcp(
        Direction::In,
        "eth0",
        CLIENT,
        40000,
        NATIP,
        80,
        tcp_flag::SYN,
        60,
    );
    let decision = engine.evaluate(&inbound, 0);
    assert_eq!(decision.verdict, Verdict::Pass);
    let translation = decision.translation.expect("RDR translation");
    assert_eq!(
        translation.destination,
        IpAddr::V4(Ipv4Addr::new(10, 0, 0, 80))
    );
    assert_eq!(translation.destination_port, 8080);
    assert_eq!(translation.source, IpAddr::V4(CLIENT));

    let reply = Packet::tcp(
        Direction::Out,
        "eth0",
        Ipv4Addr::new(10, 0, 0, 80),
        8080,
        CLIENT,
        40000,
        tcp_flag::SYN | tcp_flag::ACK,
        60,
    );
    let decision = engine.evaluate(&reply, 1);
    assert_eq!(decision.verdict, Verdict::Pass);
    let back = decision.translation.expect("reverse translation");
    assert_eq!(back.source, IpAddr::V4(NATIP));
    assert_eq!(back.source_port, 80);
    assert_eq!(back.destination, IpAddr::V4(CLIENT));
}

#[test]
fn fragments_match_address_rules_but_do_not_create_state() {
    let ruleset = parse("pass in proto tcp from any to any keep state\n").unwrap();
    let mut engine = Engine::new(ruleset);
    let fragment = Packet::tcp(Direction::In, "eth0", CLIENT, 40000, SERVER, 0, 0, 60).fragmented();
    let decision = engine.evaluate(&fragment, 0);
    assert_eq!(decision.verdict, Verdict::Pass);
    assert_eq!(decision.state, None);
    assert!(engine.states().is_empty());
}

#[test]
fn a_rule_can_request_logging() {
    let ruleset = parse("pass in on eth0 proto tcp log keep state\n").unwrap();
    let mut engine = Engine::new(ruleset);
    assert_eq!(engine.evaluate(&inbound(), 5).verdict, Verdict::Pass);
    let logs = engine.logs();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].action, crate::ast::Action::Pass);
    assert_eq!(logs[0].rule, Some(0));
    assert_eq!(logs[0].time, 5);
    assert_eq!(logs[0].source, IpAddr::V4(CLIENT));
    assert_eq!(engine.drain_logs().len(), 1);
    assert!(engine.logs().is_empty());
}

#[test]
fn a_nat_rule_without_a_usable_pool_fails_closed() {
    let ruleset = parse(
        "pass out on eth0 all keep state\n\
         nat on eth0 from any to any -> 2001:db8::1\n",
    )
    .unwrap();
    let mut engine = Engine::new(ruleset);
    let outbound = Packet::tcp(
        Direction::Out,
        "eth0",
        CLIENT,
        1234,
        SERVER,
        80,
        tcp_flag::SYN,
        60,
    );
    let decision = engine.evaluate(&outbound, 0);
    assert_eq!(decision.verdict, Verdict::Block);
    assert_eq!(decision.reason, Some(crate::abi::PFRES_MAPFAILED));
    assert_eq!(engine.counters().reason(crate::abi::PFRES_MAPFAILED), 1);
    assert_eq!(engine.logs().len(), 1);
}

#[test]
fn the_abi_status_snapshot_reflects_the_engine() {
    let ruleset = parse("pass in on eth0 proto tcp from any to any keep state\n").unwrap();
    let mut engine = Engine::new(ruleset);
    engine.evaluate(&inbound(), 0);
    let status = engine.status();
    assert!(status.running);
    assert_eq!(status.states, 1);
    assert_eq!(status.max_states, crate::state::MAX_STATES as u32);
    assert_eq!(status.fcounters_pass, 1);
    assert_eq!(status.counters[crate::abi::PFRES_MATCH], 0);
}

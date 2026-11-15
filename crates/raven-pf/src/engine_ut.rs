// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the match engine, counters and stateful shortcut.

use super::*;
use crate::address::Ipv4Addr;
use crate::ast::{Direction, tcp_flag};
use crate::parser::parse;

const CLIENT: Ipv4Addr = Ipv4Addr::new(10, 0, 0, 5);
const SERVER: Ipv4Addr = Ipv4Addr::new(192, 0, 2, 10);

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
    assert_eq!(engine.counters().in_block, 1);
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
fn default_action_can_be_pass() {
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
fn the_state_limit_is_enforced() {
    let ruleset = parse("pass in on eth0 proto udp from any to any keep state\n").unwrap();
    let mut engine = Engine::with_limit(ruleset, Timeouts::default(), 1);
    let first = Packet::udp(Direction::In, "eth0", CLIENT, 1, SERVER, 53, 40);
    let second = Packet::udp(Direction::In, "eth0", CLIENT, 2, SERVER, 53, 40);
    assert_eq!(engine.evaluate(&first, 0).verdict, Verdict::Pass);
    assert_eq!(engine.evaluate(&second, 1).verdict, Verdict::Pass);
    assert_eq!(engine.states().len(), 1);
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

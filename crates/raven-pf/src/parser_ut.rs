// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the pf-like ruleset parser.

use super::*;
use alloc::string::String;
use alloc::vec::Vec;

#[test]
fn a_full_rule_parses_every_clause() {
    let text = "pass quick in on eth0 proto tcp from 10.0.0.0/8 port 1024:65535 \
                to 192.0.2.1 port 443 keep state log\n";
    let ruleset = parse(text).unwrap();
    assert_eq!(ruleset.default_action, Action::Block);
    let rule = ruleset.rules.first().unwrap();
    assert_eq!(rule.action, Action::Pass);
    assert!(rule.quick);
    assert_eq!(rule.direction, Some(Direction::In));
    assert_eq!(rule.interface, IfaceSpec::Name("eth0".to_string()));
    assert_eq!(rule.protocol, Protocol::Tcp);
    assert_eq!(rule.source.address, AddrSpec::parse("10.0.0.0/8").unwrap());
    assert_eq!(
        rule.source.port,
        PortSpec::Range {
            start: 1024,
            end: 65535
        }
    );
    assert_eq!(
        rule.destination.address,
        AddrSpec::parse("192.0.2.1").unwrap()
    );
    assert_eq!(rule.destination.port, PortSpec::Equal(443));
    assert_eq!(rule.state, StatePolicy::Keep);
    assert!(rule.log);
}

#[test]
fn a_minimal_rule_uses_the_pf_defaults() {
    let ruleset = parse("block\n").unwrap();
    let rule = ruleset.rules.first().unwrap();
    assert_eq!(rule.action, Action::Block);
    assert!(!rule.quick);
    assert_eq!(rule.direction, None);
    assert_eq!(rule.interface, IfaceSpec::Any);
    assert_eq!(rule.protocol, Protocol::Any);
    assert_eq!(rule.source, AddrPort::any());
    assert_eq!(rule.destination, AddrPort::any());
    assert_eq!(rule.state, StatePolicy::None);
    assert!(!rule.log);
}

#[test]
fn pass_keeps_state_by_default_and_no_state_disables_it() {
    let ruleset = parse("pass in all\npass in all no state\n").unwrap();
    assert_eq!(ruleset.rules[0].state, StatePolicy::Keep);
    assert_eq!(ruleset.rules[1].state, StatePolicy::None);
}

#[test]
fn comments_and_blank_lines_are_ignored() {
    let ruleset = parse("# header\n\n  # indented\npass in all # trailing\n").unwrap();
    assert_eq!(ruleset.rules.len(), 1);
}

#[test]
fn keywords_are_case_insensitive() {
    let ruleset = parse("PASS IN ON ETH0 PROTO TCP FROM ANY TO ANY\n").unwrap();
    let rule = ruleset.rules.first().unwrap();
    assert_eq!(rule.action, Action::Pass);
    assert_eq!(rule.protocol, Protocol::Tcp);
    assert_eq!(rule.interface, IfaceSpec::Name("ETH0".to_string()));
}

#[test]
fn set_default_changes_only_the_fallback() {
    let ruleset = parse("set default pass\nblock in all\n").unwrap();
    assert_eq!(ruleset.default_action, Action::Pass);
    assert_eq!(ruleset.rules.len(), 1);
    assert_eq!(ruleset.rules[0].action, Action::Block);
}

#[test]
fn ports_accept_every_supported_spelling() {
    for text in [
        "port 22",
        "port 1:2",
        "port :1024",
        "port 1024:",
        "port <1024",
        "port >1024",
        "port !=22",
        "port any",
    ] {
        let line = format!("pass in proto tcp from any {text} to any\n");
        assert!(parse(&line).is_ok(), "{text}");
    }
}

#[test]
fn duplicate_direction_is_an_error() {
    let error = parse("pass in out all\n").unwrap_err();
    assert_eq!(error.line, 1);
    assert!(error.message.contains("duplicate direction"), "{error}");
}

#[test]
fn duplicate_from_is_an_error() {
    let error = parse("pass from any from any to any\n").unwrap_err();
    assert!(error.message.contains("duplicate `from`"), "{error}");
}

#[test]
fn unknown_keywords_are_errors_with_the_line() {
    let error = parse("pass in all\npass in proto quic from any to any\n").unwrap_err();
    assert_eq!(error.line, 2);
    assert!(error.message.contains("unknown protocol"), "{error}");
    let error = parse("pass in all\npass sideways\n").unwrap_err();
    assert_eq!(error.line, 2);
    assert!(error.message.contains("unknown keyword"), "{error}");
}

#[test]
fn malformed_addresses_and_ports_name_the_line() {
    let error = parse("pass in all\npass from 10.0.0.999 to any\n").unwrap_err();
    assert_eq!(error.line, 2);
    assert!(error.message.contains("address"), "{error}");
    let error = parse("pass from any port 70000 to any\n").unwrap_err();
    assert!(error.message.contains("port"), "{error}");
}

#[test]
fn a_bad_action_is_an_error() {
    let error = parse("allow in all\n").unwrap_err();
    assert!(
        error.message.contains("expected `pass` or `block`"),
        "{error}"
    );
}

#[test]
fn malformed_set_is_an_error() {
    assert!(parse("set default maybe\n").is_err());
    assert!(parse("set timeout tcp 10\n").is_err());
    assert!(parse("set\n").is_err());
}

#[test]
fn overlong_lines_and_names_are_rejected() {
    let long = "x".repeat(MAX_LINE + 1);
    assert!(parse(&long).is_err());
    let long_name = "i".repeat(MAX_IFACE + 1);
    assert!(parse(&format!("pass in on {long_name} all\n")).is_err());
}

#[test]
fn too_many_rules_are_rejected() {
    let mut text = String::new();
    for _ in 0..=MAX_RULES {
        text.push_str("pass in all\n");
    }
    let error = parse(&text).unwrap_err();
    assert!(error.message.contains("too many rules"), "{error}");
}

#[test]
fn random_input_never_panics() {
    let alphabet = b"abcdefghijklmnopqrstuvwxyz0123456789.:/# \n\t<>!=_";
    let mut state = 0x1234_5678_9abc_def0u64;
    for _ in 0..4000 {
        let length = (state as usize % 80) + 1;
        let mut text = String::with_capacity(length);
        for _ in 0..length {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let index = (state as usize) % alphabet.len();
            text.push(char::from(alphabet[index]));
        }
        let _ = parse(&text);
    }
}

#[test]
fn generated_valid_rules_parse() {
    let mut rules = Vec::new();
    for index in 0..200u16 {
        rules.push(format!(
            "pass in on eth{} proto tcp from 10.0.{}.0/24 port {} to any port {} keep state\n",
            index % 4,
            index % 256,
            1024 + index,
            80 + index % 100
        ));
    }
    let text = rules.join("");
    let ruleset = parse(&text).unwrap();
    assert_eq!(ruleset.rules.len(), 200);
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Printer tests: clause order and formatting of the supported subset.

use super::*;
use crate::ast::{
    Action, AddrItem, AddrPortSpec, Config, HostAddr, Policy, PortSpec, Rule, StateMode, Statement,
    TableDef, TableEntry, TableFlag,
};

fn rule() -> Rule {
    Rule {
        action: Action::Pass,
        direction: None,
        log: false,
        quick: false,
        interface: None,
        af: None,
        proto: None,
        source: AddrPortSpec::default(),
        destination: AddrPortSpec::default(),
        flags: None,
        state: StateMode::Default,
    }
}

#[test]
fn pass_all_gets_the_all_shorthand_and_defaults() {
    let mut rule = rule();
    rule.state = StateMode::Keep;
    assert_eq!(print_rule(&rule), "pass all flags any keep state");
}

#[test]
fn block_prints_its_explicit_policy() {
    let mut rule = rule();
    rule.action = Action::Block(Policy::Drop);
    rule.direction = Some(Direction::In);
    assert_eq!(print_rule(&rule), "block drop in all");
    rule.action = Action::Block(Policy::Return);
    assert_eq!(print_rule(&rule), "block return in all");
}

#[test]
fn fromto_ports_and_negation_render_like_pfctl() {
    let mut rule = rule();
    rule.direction = Some(Direction::Out);
    rule.proto = Some(Protocol::Tcp);
    rule.source = AddrPortSpec {
        items: vec![AddrItem {
            negative: false,
            address: HostAddr::Literal(String::from("10.0.0.0/8")),
            port: Some(PortSpec::Range(1000, 2000)),
        }],
    };
    rule.destination = AddrPortSpec {
        items: vec![AddrItem {
            negative: false,
            address: HostAddr::Literal(String::from("192.0.2.10")),
            port: Some(PortSpec::Eq(22)),
        }],
    };
    rule.state = StateMode::Keep;
    assert_eq!(
        print_rule(&rule),
        "pass out proto tcp from 10.0.0.0/8 port 1000:2000 to 192.0.2.10 port = 22 flags any keep state"
    );
}

#[test]
fn tables_lists_and_flag_letters() {
    let mut rule = rule();
    rule.proto = Some(Protocol::Tcp);
    rule.source = AddrPortSpec {
        items: vec![AddrItem {
            negative: true,
            address: HostAddr::Table(String::from("web")),
            port: None,
        }],
    };
    rule.flags = Some(FlagsSpec {
        set: String::from("S"),
        mask: String::from("SA"),
    });
    rule.state = StateMode::Keep;
    assert_eq!(
        print_rule(&rule),
        "pass proto tcp from ! <web> to any flags S/SA keep state"
    );
}

#[test]
fn tables_print_body_forms() {
    let table = TableDef {
        name: String::from("web"),
        flags: vec![TableFlag::Persist],
        entries: vec![
            TableEntry::Address {
                negative: false,
                address: HostAddr::Literal(String::from("10.0.0.1")),
            },
            TableEntry::Address {
                negative: true,
                address: HostAddr::Literal(String::from("10.0.0.2")),
            },
        ],
    };
    assert_eq!(
        print_statement(&Statement::Table(table)),
        "table <web> persist { 10.0.0.1 !10.0.0.2 }"
    );
    let empty = TableDef {
        name: String::from("x"),
        flags: Vec::new(),
        entries: Vec::new(),
    };
    assert_eq!(print_statement(&Statement::Table(empty)), "table <x> { }");
}

#[test]
fn config_prints_one_statement_per_line() {
    let config = Config {
        statements: vec![
            Statement::Macro {
                name: String::from("ext_if"),
                value: String::from("em0"),
            },
            Statement::Rule(rule()),
        ],
    };
    assert_eq!(print_config(&config), "ext_if = \"em0\"\npass all\n");
}

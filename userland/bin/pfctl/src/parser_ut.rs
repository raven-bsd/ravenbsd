// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Parser tests: the supported pf.conf subset and its bounded errors.

use super::*;
use alloc::string::String;
use alloc::vec;

fn parse(text: &str) -> Config {
    parse_config(text).unwrap_or_else(|error| panic!("{text}: {}", error.display()))
}

fn parse_error(text: &str) -> ParseError {
    parse_config(text).expect_err("must fail")
}

#[test]
fn macros_expand_in_order() {
    let config = parse("ext = \"em0\"\npass in on $ext all\n");
    let Statement::Rule(rule) = &config.statements[1] else {
        panic!("expected a rule");
    };
    assert_eq!(
        rule.interface.as_ref().map(|iface| iface.name.as_str()),
        Some("em0")
    );
}

#[test]
fn macros_expand_recursively_and_detect_cycles() {
    let config = parse("a = \"$b\"\nb = \"10.0.0.1\"\npass in from $a to any\n");
    let Statement::Rule(rule) = &config.statements[2] else {
        panic!("expected a rule");
    };
    assert_eq!(
        rule.source.items[0].address,
        HostAddr::Literal(String::from("10.0.0.1"))
    );
    let error = parse_config("a = \"$a\"\npass in on $a all\n").expect_err("cycle");
    assert!(error.message.contains("references itself"), "{error:?}");
}

#[test]
fn an_undefined_macro_is_line_numbered() {
    let error = parse_error("pass in all\npass in on $nope all\n");
    assert_eq!(error.line, 2);
    assert!(
        error.message.contains("is not defined"),
        "{}",
        error.message
    );
}

#[test]
fn d_macros_seed_the_table() {
    let config = parse_with(
        "pass out on $ext all\n",
        ParseOptions {
            macros: vec![(String::from("ext"), String::from("em1"))],
            ..ParseOptions::default()
        },
    )
    .expect("parse");
    let Statement::Rule(rule) = &config.statements[0] else {
        panic!("expected a rule");
    };
    assert_eq!(
        rule.interface.as_ref().map(|iface| iface.name.as_str()),
        Some("em1")
    );
}

#[test]
fn tables_parse_flags_entries_and_files() {
    let config = parse(
        "table <spam> const persist counters { 10.0.0.0/8, !10.0.0.1 }\ntable <f> file \"t.txt\"\n",
    );
    let Statement::Table(table) = &config.statements[0] else {
        panic!("expected a table");
    };
    assert_eq!(
        table.flags,
        vec![TableFlag::Const, TableFlag::Persist, TableFlag::Counters]
    );
    assert_eq!(table.entries.len(), 2);
    let TableEntry::Address { negative, .. } = &table.entries[1] else {
        panic!("expected an address");
    };
    assert!(*negative);
    let Statement::Table(file) = &config.statements[1] else {
        panic!("expected a table");
    };
    assert_eq!(file.entries, vec![TableEntry::File(String::from("t.txt"))]);
}

#[test]
fn options_parse_and_block_policy_sets_the_default() {
    let config = parse(
        "set block-policy return\nset fail-policy drop\nset require-order no\n\
         set state-policy if-bound\nset skip on ! lo0\nset optimization profile\n\
         set loginterface em0\nset hostid 7\nset timeout tcp.first 120\n\
         set limit { states 100, frags 200 }\nblock in all\n",
    );
    assert_eq!(config.statements.len(), 11);
    let Statement::Rule(rule) = &config.statements[10] else {
        panic!("expected a rule");
    };
    assert_eq!(rule.action, Action::Block(Policy::Return));
    match &config.statements[9] {
        Statement::Option(OptionDirective::Limit(pairs)) => {
            assert_eq!(
                pairs,
                &vec![
                    (String::from("states"), 100u32),
                    (String::from("frags"), 200u32)
                ]
            );
        }
        other => panic!("expected a limit option, got {other:?}"),
    }
}

#[test]
fn rules_parse_every_supported_clause() {
    let config = parse(
        "pass in log quick on ! em0 inet6 proto tcp from any to 2001:db8::1 port 443 \
         flags S/SA keep state\n",
    );
    let Statement::Rule(rule) = &config.statements[0] else {
        panic!("expected a rule");
    };
    assert!(rule.log && rule.quick);
    assert_eq!(rule.af, Some(AddressFamily::Inet6));
    assert_eq!(rule.proto, Some(Protocol::Tcp));
    assert!(rule.interface.as_ref().expect("interface").negative);
    assert_eq!(rule.state, StateMode::Keep);
    assert_eq!(
        rule.flags
            .as_ref()
            .map(|flags| (flags.set.as_str(), flags.mask.as_str())),
        Some(("S", "SA"))
    );
}

#[test]
fn any_with_a_port_keeps_the_port() {
    let config = parse("pass in proto tcp to any port { 80, 443 } flags S/SA\n");
    let Statement::Rule(rule) = &config.statements[0] else {
        panic!("expected a rule");
    };
    assert_eq!(rule.destination.items.len(), 1);
    assert_eq!(
        rule.destination.items[0].port,
        Some(PortSpec::Set(vec![PortSpec::Eq(80), PortSpec::Eq(443)]))
    );
}

#[test]
fn nat_rdr_and_no_forms_parse() {
    let config = parse(
        "nat pass on em0 proto tcp from 10.0.0.0/8 to any -> 203.0.113.1 port 8080 round-robin\n\
         rdr on em0 proto tcp from any to any port = 80 -> 192.0.2.10\n\
         no nat on em0 proto tcp from any to any\n\
         anchor \"spam\"\n",
    );
    let Statement::Nat(nat) = &config.statements[0] else {
        panic!("expected nat");
    };
    assert!(nat.pass);
    let redirect = nat.redirect.as_ref().expect("redirect");
    assert_eq!(redirect.port, Some(8080));
    assert_eq!(redirect.pool, Some(PoolOption::RoundRobin));
    let Statement::Nat(rdr) = &config.statements[1] else {
        panic!("expected rdr");
    };
    assert_eq!(rdr.kind, NatKind::Rdr);
    let Statement::Nat(none) = &config.statements[2] else {
        panic!("expected no nat");
    };
    assert_eq!(none.kind, NatKind::NoNat);
    assert!(none.redirect.is_none());
    let Statement::Anchor(name) = &config.statements[3] else {
        panic!("expected an anchor");
    };
    assert_eq!(name, "spam");
}

#[test]
fn a_nat_rule_without_a_target_fails() {
    let error = parse_error("nat on em0 from any to any\n");
    assert!(
        error.message.contains("requires `-> address'"),
        "{}",
        error.message
    );
}

#[test]
fn a_no_rule_with_a_target_fails() {
    let error = parse_error("no nat on em0 from any to any -> 10.0.0.1\n");
    assert!(
        error.message.contains("does not take `->'"),
        "{}",
        error.message
    );
}

#[test]
fn include_without_a_loader_is_kept() {
    let config = parse("include \"other.conf\"\n");
    assert_eq!(
        config.statements,
        vec![Statement::Include(String::from("other.conf"))]
    );
}

#[test]
fn unknown_directives_and_junk_are_bounded_errors() {
    let error = parse_error("frobnicate in all\n");
    assert!(
        error.message.contains("unknown directive"),
        "{}",
        error.message
    );
    let error = parse_error("pass in all junk\n");
    assert!(
        error.message.contains("unexpected token"),
        "{}",
        error.message
    );
    let error = parse_error("pass in proto sctp all\n");
    assert!(
        error.message.contains("unsupported protocol"),
        "{}",
        error.message
    );
}

#[test]
fn ports_ranges_and_operators_are_validated() {
    let error = parse_error("pass in from any port 0 to any\n");
    assert!(error.message.contains("invalid port"), "{}", error.message);
    let error = parse_error("pass in from any port 2000:1000 to any\n");
    assert!(error.message.contains("inverted"), "{}", error.message);
    let error = parse_error("pass in from any port > 80:90 to any\n");
    assert!(
        error.message.contains("comparison operator"),
        "{}",
        error.message
    );
    let config = parse("pass in from any port 1000:2000 to any\n");
    let Statement::Rule(rule) = &config.statements[0] else {
        panic!("expected a rule");
    };
    assert_eq!(rule.source.items[0].port, Some(PortSpec::Range(1000, 2000)));
}

#[test]
fn flags_must_be_a_valid_subset() {
    let error = parse_error("pass in proto tcp flags A/S keep state\n");
    assert!(error.message.contains("subset"), "{}", error.message);
    let error = parse_error("pass in proto tcp flags Z/SA keep state\n");
    assert!(
        error.message.contains("invalid TCP flag"),
        "{}",
        error.message
    );
    let config = parse("pass in proto tcp flags any\n");
    let Statement::Rule(rule) = &config.statements[0] else {
        panic!("expected a rule");
    };
    assert!(rule.flags.is_none(), "`flags any' is the implicit form");
    let config = parse("pass in proto tcp flags /SA\n");
    let Statement::Rule(rule) = &config.statements[0] else {
        panic!("expected a rule");
    };
    assert_eq!(
        rule.flags
            .as_ref()
            .map(|flags| (flags.set.as_str(), flags.mask.as_str())),
        Some(("", "SA"))
    );
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Canonical pf.conf printer.
//!
//! The output rules are ports of the FreeBSD donor `sbin/pfctl/pfctl_parser.c`
//! at `133344405f` (BSD-2-Clause): `print_rule` (clause order and defaults),
//! `print_fromto` (the ` all` shorthand), `print_port`/`print_op` (port
//! operators), `print_flags` (TCP flag letters) and `print_tabledef` (table
//! body form). Divergences: address lists are kept as `{ a, b }` instead of
//! being expanded into separate rules, and ports are always numeric.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::ast::{
    Action, AddrItem, AddrPortSpec, AddressFamily, Config, Direction, FlagsSpec, HostAddr,
    IfaceRef, NatKind, NatRule, OptionDirective, Policy, PolicyValue, PoolOption, PortSpec,
    Protocol, Rule, StateMode, StatePolicyValue, Statement, TableDef, TableEntry, TableFlag,
};

/// Print a whole configuration, one statement per line, with a final newline.
#[must_use]
pub fn print_config(config: &Config) -> String {
    let mut out = String::new();
    for statement in &config.statements {
        out.push_str(&print_statement(statement));
        out.push('\n');
    }
    out
}

/// Print one statement (may span several lines for name/value option pairs).
#[must_use]
pub fn print_statement(statement: &Statement) -> String {
    match statement {
        Statement::Macro { name, value } => format!("{name} = \"{value}\""),
        Statement::Table(table) => print_table(table),
        Statement::Option(option) => print_option(option),
        Statement::Rule(rule) => print_rule(rule),
        Statement::Nat(rule) => print_nat(rule),
        Statement::Anchor(name) => format!("anchor \"{name}\""),
        Statement::Include(path) => format!("include \"{path}\""),
    }
}

fn print_table(table: &TableDef) -> String {
    let mut out = format!("table <{}>", table.name);
    for flag in &table.flags {
        out.push_str(match flag {
            TableFlag::Const => " const",
            TableFlag::Persist => " persist",
            TableFlag::Counters => " counters",
        });
    }
    let mut addresses: Vec<&TableEntry> = Vec::new();
    for entry in &table.entries {
        match entry {
            TableEntry::File(path) => {
                out.push_str(&format!(" file \"{path}\""));
            }
            TableEntry::Address { .. } => addresses.push(entry),
        }
    }
    if !addresses.is_empty() {
        out.push_str(" {");
        for entry in addresses {
            let TableEntry::Address { negative, address } = entry else {
                continue;
            };
            out.push_str(if *negative { " !" } else { " " });
            out.push_str(&print_host(address));
        }
        out.push_str(" }");
    } else if table.entries.is_empty() {
        out.push_str(" { }");
    }
    out
}

fn print_option(option: &OptionDirective) -> String {
    match option {
        OptionDirective::BlockPolicy(value) => {
            format!("set block-policy {}", policy_word(*value))
        }
        OptionDirective::FailPolicy(value) => {
            format!("set fail-policy {}", policy_word(*value))
        }
        OptionDirective::RequireOrder(value) => {
            format!("set require-order {}", if *value { "yes" } else { "no" })
        }
        OptionDirective::StatePolicy(value) => match value {
            StatePolicyValue::Floating => String::from("set state-policy floating"),
            StatePolicyValue::IfBound => String::from("set state-policy if-bound"),
        },
        OptionDirective::Skip(iface) => format!("set skip on {}", print_iface(iface)),
        OptionDirective::Optimization(value) => format!("set optimization {value}"),
        OptionDirective::LogInterface(value) => format!("set loginterface {value}"),
        OptionDirective::HostId(value) => format!("set hostid {value}"),
        OptionDirective::Timeout(pairs) => print_pairs("timeout", pairs),
        OptionDirective::Limit(pairs) => print_pairs("limit", pairs),
    }
}

fn print_pairs(kind: &str, pairs: &[(String, u32)]) -> String {
    pairs
        .iter()
        .map(|(name, value)| format!("set {kind} {name} {value}"))
        .collect::<Vec<String>>()
        .join("\n")
}

fn policy_word(value: PolicyValue) -> &'static str {
    match value {
        PolicyValue::Drop => "drop",
        PolicyValue::Return => "return",
    }
}

fn print_rule(rule: &Rule) -> String {
    let mut out = String::new();
    out.push_str(&action_words(&rule.action));
    if let Some(direction) = rule.direction {
        out.push_str(match direction {
            Direction::In => " in",
            Direction::Out => " out",
        });
    }
    if rule.log {
        out.push_str(" log");
    }
    if rule.quick {
        out.push_str(" quick");
    }
    if let Some(iface) = &rule.interface {
        out.push_str(" on ");
        out.push_str(&print_iface(iface));
    }
    if let Some(af) = rule.af {
        out.push_str(match af {
            AddressFamily::Inet => " inet",
            AddressFamily::Inet6 => " inet6",
        });
    }
    if let Some(proto) = rule.proto {
        out.push_str(" proto ");
        out.push_str(&print_proto(proto));
    }
    push_fromto(&mut out, &rule.source, &rule.destination);
    if let Some(flags) = &rule.flags {
        out.push_str(" flags ");
        out.push_str(&format_flags(flags));
    } else if matches!(rule.action, Action::Pass | Action::Match)
        && matches!(rule.proto, None | Some(Protocol::Tcp))
        && matches!(
            rule.state,
            StateMode::Keep | StateMode::Modulate | StateMode::Synproxy
        )
    {
        out.push_str(" flags any");
    }
    match rule.state {
        StateMode::Keep => out.push_str(" keep state"),
        StateMode::Modulate => out.push_str(" modulate state"),
        StateMode::Synproxy => out.push_str(" synproxy state"),
        StateMode::None if rule.action == Action::Pass => out.push_str(" no state"),
        StateMode::None | StateMode::Default => {}
    }
    out
}

fn action_words(action: &Action) -> String {
    match action {
        Action::Pass => String::from("pass"),
        Action::Match => String::from("match"),
        Action::Scrub => String::from("scrub"),
        Action::NoScrub => String::from("no scrub"),
        Action::Block(policy) => format!(
            "block {}",
            match policy {
                Policy::Drop => "drop",
                Policy::Return => "return",
            }
        ),
    }
}

fn print_nat(rule: &NatRule) -> String {
    let mut out = String::from(rule.kind.name());
    if rule.pass {
        out.push_str(" pass");
    }
    if rule.log {
        out.push_str(" log");
    }
    if rule.quick {
        out.push_str(" quick");
    }
    if let Some(iface) = &rule.interface {
        out.push_str(" on ");
        out.push_str(&print_iface(iface));
    }
    if let Some(af) = rule.af {
        out.push_str(match af {
            AddressFamily::Inet => " inet",
            AddressFamily::Inet6 => " inet6",
        });
    }
    if let Some(proto) = rule.proto {
        out.push_str(" proto ");
        out.push_str(&print_proto(proto));
    }
    push_fromto(&mut out, &rule.source, &rule.destination);
    if let Some(redirect) = &rule.redirect {
        out.push_str(" -> ");
        out.push_str(&print_host(&redirect.address));
        if let Some(port) = redirect.port {
            out.push_str(&format!(" port {port}"));
        }
        if let Some(pool) = redirect.pool {
            out.push_str(match pool {
                PoolOption::RoundRobin => " round-robin",
                PoolOption::SourceHash => " source-hash",
                PoolOption::Bitmask => " bitmask",
            });
        }
    }
    let _ = matches!(
        rule.kind,
        NatKind::NoNat | NatKind::NoRdr | NatKind::NoBinat
    );
    out
}

fn push_fromto(out: &mut String, source: &AddrPortSpec, destination: &AddrPortSpec) {
    if source.is_any() && destination.is_any() {
        out.push_str(" all");
        return;
    }
    out.push_str(" from ");
    out.push_str(&print_addrspec(source));
    out.push_str(" to ");
    out.push_str(&print_addrspec(destination));
}

fn print_addrspec(spec: &AddrPortSpec) -> String {
    if spec.items.is_empty() {
        return String::from("any");
    }
    if spec.items.len() == 1 {
        return print_addr_item(&spec.items[0]);
    }
    let mut out = String::from("{ ");
    for (index, item) in spec.items.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        out.push_str(&print_addr_item(item));
    }
    out.push_str(" }");
    out
}

fn print_addr_item(item: &AddrItem) -> String {
    let mut out = String::new();
    if item.negative {
        out.push_str("! ");
    }
    out.push_str(&print_host(&item.address));
    if let Some(port) = &item.port {
        out.push_str(" port ");
        out.push_str(&print_port(port));
    }
    out
}

fn print_host(host: &HostAddr) -> String {
    match host {
        HostAddr::Any => String::from("any"),
        HostAddr::Literal(text) => text.clone(),
        HostAddr::Table(name) => format!("<{name}>"),
    }
}

fn print_port(port: &PortSpec) -> String {
    match port {
        PortSpec::Eq(value) => format!("= {value}"),
        PortSpec::Ne(value) => format!("!= {value}"),
        PortSpec::Lt(value) => format!("< {value}"),
        PortSpec::Le(value) => format!("<= {value}"),
        PortSpec::Gt(value) => format!("> {value}"),
        PortSpec::Ge(value) => format!(">= {value}"),
        PortSpec::Range(low, high) => format!("{low}:{high}"),
        PortSpec::Set(items) => {
            let mut out = String::from("{ ");
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                // A bare list entry prints as the number (pf.conf list syntax).
                match item {
                    PortSpec::Eq(value) => out.push_str(&format!("{value}")),
                    other => out.push_str(&print_port(other)),
                }
            }
            out.push_str(" }");
            out
        }
    }
}

fn format_flags(flags: &FlagsSpec) -> String {
    format!("{}/{}", flags.set, flags.mask)
}

fn print_iface(iface: &IfaceRef) -> String {
    if iface.negative {
        format!("! {}", iface.name)
    } else {
        iface.name.clone()
    }
}

fn print_proto(proto: Protocol) -> String {
    match proto.name() {
        Some(name) => name.to_string(),
        None => match proto {
            Protocol::Number(value) => value.to_string(),
            _ => unreachable!("named protocols are covered by Protocol::name"),
        },
    }
}

#[cfg(test)]
#[path = "print_ut.rs"]
mod tests;

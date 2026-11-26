// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Parser for the pf.conf-compatible ruleset subset.
//!
//! Donor: the pf.conf grammar handled by `usr.sbin/pfctl` (absent from the pruned donor
//! snapshot; see deviation D-pf-layout). The accepted subset is documented in `docs/pf.md`:
//! `pass`/`block` rules with `quick`, `in`/`out`, `on`, `inet`/`inet6`, `proto`,
//! `from`/`to` with addresses, tables and ports, `keep`/`no state` and `log`;
//! `set default`; `table <name> { ... }`; and single-line `nat`/`rdr` rules.
//!
//! Every malformed construct is an error (fail-closed); input is bounded by [`MAX_LINE`],
//! [`MAX_RULES`], [`MAX_TABLES`] and [`MAX_NAT_RULES`].

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use crate::address::{AddrSpec, IpAddr, PortSpec};
use crate::ast::{
    Action, AddrPort, Direction, Family, IfaceSpec, Protocol, Rule, Ruleset, StatePolicy,
};
use crate::nat::{MAX_NAT_RULES, NatAction, NatRule, Pool, PoolKind};
use crate::table::{Table, check_table_capacity};

/// Largest accepted rule count.
pub const MAX_RULES: usize = 4096;
/// Largest accepted line length in bytes.
pub const MAX_LINE: usize = 512;
/// Largest accepted interface name length.
pub const MAX_IFACE: usize = 32;
/// Largest accepted table name length.
pub const MAX_TABLE_NAME: usize = 32;

/// One parse failure with its 1-based source line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// 1-based source line.
    pub line: usize,
    /// Human-readable reason.
    pub message: String,
}

impl ParseError {
    fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

/// Parse a complete ruleset.
///
/// # Errors
///
/// Returns [`ParseError`] for an unknown keyword, a malformed address/port/table, a duplicate
/// `from`/`to` clause, a family conflict, an unknown table, an over-long line/name or a
/// configured bound.
pub fn parse(text: &str) -> Result<Ruleset, ParseError> {
    let mut lines: Vec<(usize, String)> = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        if raw.len() > MAX_LINE {
            return Err(ParseError::new(line, "line is longer than 512 bytes"));
        }
        let without_comment = raw.split('#').next().unwrap_or_default().trim();
        if without_comment.is_empty() {
            continue;
        }
        lines.push((line, normalize_braces(without_comment)));
    }

    let mut tables: Vec<Table> = Vec::new();
    for (line, text) in &lines {
        let tokens: Vec<&str> = text.split_whitespace().collect();
        if tokens
            .first()
            .is_some_and(|token| token.eq_ignore_ascii_case("table"))
        {
            parse_table(&mut tables, &tokens, *line)?;
        }
    }

    let mut ruleset = Ruleset::new();
    ruleset.tables = tables;
    for (line, text) in &lines {
        let tokens: Vec<&str> = text.split_whitespace().collect();
        let Some(first) = tokens.first().copied() else {
            continue;
        };
        if first.eq_ignore_ascii_case("table") {
            continue;
        }
        if first.eq_ignore_ascii_case("set") {
            parse_set(&mut ruleset, &tokens, *line)?;
            continue;
        }
        if first.eq_ignore_ascii_case("nat") || first.eq_ignore_ascii_case("rdr") {
            let rule = parse_nat_rule(&tokens, *line, &ruleset.tables)?;
            if !ruleset.nat.push(rule) {
                return Err(ParseError::new(
                    *line,
                    format!("too many NAT rules (limit {MAX_NAT_RULES})"),
                ));
            }
            continue;
        }
        if ruleset.rules.len() >= MAX_RULES {
            return Err(ParseError::new(
                *line,
                format!("too many rules (limit {MAX_RULES})"),
            ));
        }
        let rule = parse_rule(&tokens, *line, &ruleset.tables)?;
        ruleset.rules.push(rule);
    }
    Ok(ruleset)
}

fn normalize_braces(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for character in text.chars() {
        if matches!(character, '{' | '}' | ',') {
            out.push(' ');
            out.push(character);
            out.push(' ');
        } else {
            out.push(character);
        }
    }
    out
}

fn parse_set(ruleset: &mut Ruleset, tokens: &[&str], line: usize) -> Result<(), ParseError> {
    match tokens {
        [_, key, value] if key.eq_ignore_ascii_case("default") => {
            ruleset.default_action = parse_action(value).ok_or_else(|| {
                ParseError::new(line, format!("unknown default action `{value}`"))
            })?;
            Ok(())
        }
        [_, key, _] if key.eq_ignore_ascii_case("timeout") => Err(ParseError::new(
            line,
            "timeouts are configured through the engine API, not the ruleset",
        )),
        _ => Err(ParseError::new(line, "expected `set default pass|block`")),
    }
}

fn parse_table(tables: &mut Vec<Table>, tokens: &[&str], line: usize) -> Result<(), ParseError> {
    if tokens.len() < 4 {
        return Err(ParseError::new(
            line,
            "expected `table <name> { address, ... }`",
        ));
    }
    let name =
        parse_table_name(tokens[1]).ok_or_else(|| ParseError::new(line, "invalid table name"))?;
    if tables
        .iter()
        .any(|table| table.name.eq_ignore_ascii_case(name))
    {
        return Err(ParseError::new(line, format!("duplicate table `{name}`")));
    }
    check_table_capacity(tables)
        .map_err(|error| ParseError::new(line, format!("table: {error}")))?;
    if tokens[2] != "{" || tokens.last().copied() != Some("}") {
        return Err(ParseError::new(
            line,
            "expected `{` and `}` around the table",
        ));
    }
    let mut table = Table::new(name);
    for token in &tokens[3..tokens.len() - 1] {
        if *token == "," {
            continue;
        }
        if token.starts_with('<') {
            return Err(ParseError::new(line, "a table cannot contain a table"));
        }
        let spec = AddrSpec::parse(token)
            .map_err(|error| ParseError::new(line, format!("table entry: {error}")))?;
        table
            .add(spec)
            .map_err(|error| ParseError::new(line, format!("table: {error}")))?;
    }
    tables.push(table);
    Ok(())
}

fn parse_table_name(text: &str) -> Option<&str> {
    let name = text.strip_prefix('<')?.strip_suffix('>')?;
    if name.is_empty() || name.len() > MAX_TABLE_NAME {
        return None;
    }
    if !name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return None;
    }
    Some(name)
}

fn find_table(tables: &[Table], text: &str) -> Result<u16, ()> {
    let name = parse_table_name(text).ok_or(())?;
    tables
        .iter()
        .position(|table| table.name.eq_ignore_ascii_case(name))
        .and_then(|index| u16::try_from(index).ok())
        .ok_or(())
}

fn parse_rule(tokens: &[&str], line: usize, tables: &[Table]) -> Result<Rule, ParseError> {
    let action = parse_action(tokens[0]).ok_or_else(|| {
        ParseError::new(
            line,
            format!("expected `pass` or `block`, found `{}`", tokens[0]),
        )
    })?;
    let mut rule = Rule {
        action,
        quick: false,
        direction: None,
        interface: IfaceSpec::Any,
        family: None,
        protocol: Protocol::Any,
        source: AddrPort::any(),
        destination: AddrPort::any(),
        state: if action == Action::Pass {
            StatePolicy::Keep
        } else {
            StatePolicy::None
        },
        log: false,
    };
    let mut saw_source = false;
    let mut saw_destination = false;
    let mut index = 1;
    while index < tokens.len() {
        let token = tokens[index];
        if token.eq_ignore_ascii_case("quick") {
            rule.quick = true;
            index += 1;
        } else if token.eq_ignore_ascii_case("in") {
            set_direction(&mut rule, Direction::In, line)?;
            index += 1;
        } else if token.eq_ignore_ascii_case("out") {
            set_direction(&mut rule, Direction::Out, line)?;
            index += 1;
        } else if token.eq_ignore_ascii_case("on") {
            let name = tokens
                .get(index + 1)
                .ok_or_else(|| ParseError::new(line, "`on` needs an interface"))?;
            rule.interface = parse_interface(name, line)?;
            index += 2;
        } else if token.eq_ignore_ascii_case("inet") {
            set_family(&mut rule, Family::Inet, line)?;
            index += 1;
        } else if token.eq_ignore_ascii_case("inet6") {
            set_family(&mut rule, Family::Inet6, line)?;
            index += 1;
        } else if token.eq_ignore_ascii_case("proto") {
            let name = tokens
                .get(index + 1)
                .ok_or_else(|| ParseError::new(line, "`proto` needs a protocol"))?;
            rule.protocol = parse_protocol(name)
                .ok_or_else(|| ParseError::new(line, format!("unknown protocol `{name}`")))?;
            index += 2;
        } else if token.eq_ignore_ascii_case("from") {
            if saw_source {
                return Err(ParseError::new(line, "duplicate `from` clause"));
            }
            saw_source = true;
            let (source, used) = parse_addr_port(tokens, index + 1, line, tables)?;
            rule.source = source;
            index += 1 + used;
        } else if token.eq_ignore_ascii_case("to") {
            if saw_destination {
                return Err(ParseError::new(line, "duplicate `to` clause"));
            }
            saw_destination = true;
            let (destination, used) = parse_addr_port(tokens, index + 1, line, tables)?;
            rule.destination = destination;
            index += 1 + used;
        } else if token.eq_ignore_ascii_case("keep") {
            if !tokens
                .get(index + 1)
                .is_some_and(|next| next.eq_ignore_ascii_case("state"))
            {
                return Err(ParseError::new(line, "expected `keep state`"));
            }
            rule.state = StatePolicy::Keep;
            index += 2;
        } else if token.eq_ignore_ascii_case("no") {
            if !tokens
                .get(index + 1)
                .is_some_and(|next| next.eq_ignore_ascii_case("state"))
            {
                return Err(ParseError::new(line, "expected `no state`"));
            }
            rule.state = StatePolicy::None;
            index += 2;
        } else if token.eq_ignore_ascii_case("log") {
            rule.log = true;
            index += 1;
        } else if token.eq_ignore_ascii_case("all") {
            index += 1;
        } else {
            return Err(ParseError::new(line, format!("unknown keyword `{token}`")));
        }
    }
    resolve_family(&mut rule, tables, line)?;
    Ok(rule)
}

fn set_direction(rule: &mut Rule, direction: Direction, line: usize) -> Result<(), ParseError> {
    if rule.direction.is_some() {
        return Err(ParseError::new(line, "duplicate direction"));
    }
    rule.direction = Some(direction);
    Ok(())
}

fn set_family(rule: &mut Rule, family: Family, line: usize) -> Result<(), ParseError> {
    if rule.family.is_some() {
        return Err(ParseError::new(line, "duplicate address family"));
    }
    rule.family = Some(family);
    Ok(())
}

fn resolve_family(rule: &mut Rule, tables: &[Table], line: usize) -> Result<(), ParseError> {
    let source = spec_family(rule.source.address, tables);
    let destination = spec_family(rule.destination.address, tables);
    if let (Some(left), Some(right)) = (source, destination)
        && left != right
    {
        return Err(ParseError::new(
            line,
            "source and destination address families differ",
        ));
    }
    let derived = source.or(destination);
    match (rule.family, derived) {
        (Some(want), Some(got)) if want != got => Err(ParseError::new(
            line,
            "address family conflicts with `inet`/`inet6`",
        )),
        (None, Some(got)) => {
            rule.family = Some(got);
            Ok(())
        }
        _ => Ok(()),
    }
}

fn spec_family(spec: AddrSpec, tables: &[Table]) -> Option<Family> {
    match spec {
        AddrSpec::Host(address) => Some(family_of(address)),
        AddrSpec::Network { network, .. } => Some(family_of(network)),
        AddrSpec::Table(index) => {
            let table = tables.get(usize::from(index))?;
            let mut family = None;
            for entry in &table.entries {
                let entry_family = spec_family(*entry, tables)?;
                if let Some(known) = family
                    && known != entry_family
                {
                    return None;
                }
                family = Some(entry_family);
            }
            family
        }
        AddrSpec::Any => None,
    }
}

fn family_of(address: IpAddr) -> Family {
    if address.is_v4() {
        Family::Inet
    } else {
        Family::Inet6
    }
}

fn parse_action(text: &str) -> Option<Action> {
    if text.eq_ignore_ascii_case("pass") {
        Some(Action::Pass)
    } else if text.eq_ignore_ascii_case("block") {
        Some(Action::Block)
    } else {
        None
    }
}

fn parse_protocol(text: &str) -> Option<Protocol> {
    if text.eq_ignore_ascii_case("any") || text.eq_ignore_ascii_case("all") {
        Some(Protocol::Any)
    } else if text.eq_ignore_ascii_case("tcp") {
        Some(Protocol::Tcp)
    } else if text.eq_ignore_ascii_case("udp") {
        Some(Protocol::Udp)
    } else if text.eq_ignore_ascii_case("icmp") {
        Some(Protocol::Icmp)
    } else if text.eq_ignore_ascii_case("icmp6") || text.eq_ignore_ascii_case("icmpv6") {
        Some(Protocol::Icmpv6)
    } else {
        None
    }
}

fn parse_interface(text: &str, line: usize) -> Result<IfaceSpec, ParseError> {
    if text.eq_ignore_ascii_case("any") {
        return Ok(IfaceSpec::Any);
    }
    if text.is_empty() || text.len() > MAX_IFACE {
        return Err(ParseError::new(
            line,
            "interface name must be 1..=32 characters",
        ));
    }
    if !text
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(ParseError::new(line, "invalid interface name"));
    }
    Ok(IfaceSpec::Name(text.to_string()))
}

fn parse_addr_port(
    tokens: &[&str],
    start: usize,
    line: usize,
    tables: &[Table],
) -> Result<(AddrPort, usize), ParseError> {
    let address_text = tokens
        .get(start)
        .ok_or_else(|| ParseError::new(line, "expected an address"))?;
    let address = if address_text.starts_with('<') {
        let index = find_table(tables, address_text)
            .map_err(|()| ParseError::new(line, format!("unknown table `{address_text}`")))?;
        AddrSpec::Table(index)
    } else {
        AddrSpec::parse(address_text)
            .map_err(|error| ParseError::new(line, format!("address: {error}")))?
    };
    let mut used = 1;
    let mut port = PortSpec::Any;
    if tokens
        .get(start + 1)
        .is_some_and(|token| token.eq_ignore_ascii_case("port"))
    {
        let port_text = tokens
            .get(start + 2)
            .ok_or_else(|| ParseError::new(line, "`port` needs a value"))?;
        port = PortSpec::parse(port_text)
            .map_err(|error| ParseError::new(line, format!("port: {error}")))?;
        used = 3;
    }
    Ok((AddrPort { address, port }, used))
}

fn parse_nat_rule(tokens: &[&str], line: usize, tables: &[Table]) -> Result<NatRule, ParseError> {
    let action = if tokens[0].eq_ignore_ascii_case("nat") {
        NatAction::Nat
    } else {
        NatAction::Rdr
    };
    let mut rule = NatRule {
        action,
        direction: Some(match action {
            NatAction::Nat => Direction::Out,
            NatAction::Rdr => Direction::In,
        }),
        interface: IfaceSpec::Any,
        protocol: Protocol::Any,
        source: AddrPort::any(),
        destination: AddrPort::any(),
        pool: Pool::new(),
        log: false,
    };
    let mut saw_source = false;
    let mut saw_destination = false;
    let mut saw_arrow = false;
    let mut index = 1;
    while index < tokens.len() {
        let token = tokens[index];
        if token.eq_ignore_ascii_case("on") {
            let name = tokens
                .get(index + 1)
                .ok_or_else(|| ParseError::new(line, "`on` needs an interface"))?;
            rule.interface = parse_interface(name, line)?;
            index += 2;
        } else if token.eq_ignore_ascii_case("proto") {
            let name = tokens
                .get(index + 1)
                .ok_or_else(|| ParseError::new(line, "`proto` needs a protocol"))?;
            rule.protocol = parse_protocol(name)
                .ok_or_else(|| ParseError::new(line, format!("unknown protocol `{name}`")))?;
            index += 2;
        } else if token.eq_ignore_ascii_case("from") {
            if saw_source {
                return Err(ParseError::new(line, "duplicate `from` clause"));
            }
            saw_source = true;
            let (source, used) = parse_addr_port(tokens, index + 1, line, tables)?;
            rule.source = source;
            index += 1 + used;
        } else if token.eq_ignore_ascii_case("to") {
            if saw_destination {
                return Err(ParseError::new(line, "duplicate `to` clause"));
            }
            saw_destination = true;
            let (destination, used) = parse_addr_port(tokens, index + 1, line, tables)?;
            rule.destination = destination;
            index += 1 + used;
        } else if token == "->" {
            saw_arrow = true;
            index += 1;
            index = parse_pool(&mut rule.pool, tokens, index, line)?;
        } else if token.eq_ignore_ascii_case("round-robin") {
            rule.pool.kind = PoolKind::RoundRobin;
            index += 1;
        } else if token.eq_ignore_ascii_case("random") {
            rule.pool.kind = PoolKind::Random;
            index += 1;
        } else if token.eq_ignore_ascii_case("source-hash") {
            rule.pool.kind = PoolKind::SourceHash;
            index += 1;
        } else if token.eq_ignore_ascii_case("bitmask") {
            rule.pool.kind = PoolKind::Bitmask;
            index += 1;
        } else if token.eq_ignore_ascii_case("static-port") {
            rule.pool.static_port = true;
            index += 1;
        } else if token.eq_ignore_ascii_case("log") {
            rule.log = true;
            index += 1;
        } else {
            return Err(ParseError::new(
                line,
                format!("unknown NAT keyword `{token}`"),
            ));
        }
    }
    if !saw_arrow || rule.pool.addresses.is_empty() {
        return Err(ParseError::new(line, "a NAT rule needs `-> address`"));
    }
    if rule.pool.addresses.len() > crate::nat::MAX_POOL_ADDRESSES {
        return Err(ParseError::new(line, "too many pool addresses"));
    }
    Ok(rule)
}

fn parse_pool(
    pool: &mut Pool,
    tokens: &[&str],
    mut index: usize,
    line: usize,
) -> Result<usize, ParseError> {
    if tokens.get(index).copied() == Some("{") {
        index += 1;
        loop {
            let token = tokens
                .get(index)
                .copied()
                .ok_or_else(|| ParseError::new(line, "unterminated pool list"))?;
            index += 1;
            if token == "}" {
                break;
            }
            if token == "," {
                continue;
            }
            add_pool_address(pool, token, line)?;
        }
    } else {
        let token = tokens
            .get(index)
            .copied()
            .ok_or_else(|| ParseError::new(line, "`->` needs an address"))?;
        index += 1;
        add_pool_address(pool, token, line)?;
    }
    if tokens
        .get(index)
        .is_some_and(|token| token.eq_ignore_ascii_case("port"))
    {
        let port_text = tokens
            .get(index + 1)
            .ok_or_else(|| ParseError::new(line, "`port` needs a value"))?;
        let port = PortSpec::parse(port_text)
            .map_err(|error| ParseError::new(line, format!("port: {error}")))?;
        pool.proxy_port = port.span();
        if pool.proxy_port.is_none() {
            return Err(ParseError::new(
                line,
                "the pool port must be a port or a port range",
            ));
        }
        index += 2;
    }
    Ok(index)
}

fn add_pool_address(pool: &mut Pool, text: &str, line: usize) -> Result<(), ParseError> {
    if pool.addresses.len() >= crate::nat::MAX_POOL_ADDRESSES {
        return Err(ParseError::new(line, "too many pool addresses"));
    }
    let spec =
        AddrSpec::parse(text).map_err(|error| ParseError::new(line, format!("pool: {error}")))?;
    let AddrSpec::Host(address) = spec else {
        return Err(ParseError::new(line, "a pool needs host addresses"));
    };
    pool.addresses.push(address);
    Ok(())
}

#[cfg(test)]
#[path = "parser_ut.rs"]
mod tests;

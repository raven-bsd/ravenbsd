// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Parser for the pf-like ruleset language.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use crate::address::{AddrSpec, PortSpec};
use crate::ast::{
    Action, AddrPort, Direction, IfaceSpec, NatRule, Protocol, Rule, Ruleset, StatePolicy, Table,
};

/// Largest accepted rule count.
pub const MAX_RULES: usize = 4096;
/// Largest accepted line length in bytes.
pub const MAX_LINE: usize = 512;
/// Largest accepted interface name length.
pub const MAX_IFACE: usize = 32;

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
/// Returns [`ParseError`] for an unknown keyword, a malformed address/port, a duplicate
/// `from`/`to` clause, an over-long line/name or more than [`MAX_RULES`] rules.
pub fn parse(text: &str) -> Result<Ruleset, ParseError> {
    let mut ruleset = Ruleset::new();
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        if raw.len() > MAX_LINE {
            return Err(ParseError::new(line, "line is longer than 512 bytes"));
        }
        let without_comment = raw.split('#').next().unwrap_or_default().trim();
        if without_comment.is_empty() {
            continue;
        }
        let brace_text = without_comment.replace('{', " { ").replace('}', " } ");
        let tokens: Vec<&str> = brace_text.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }
        if tokens[0].eq_ignore_ascii_case("set") {
            parse_set(&mut ruleset, &tokens, line)?;
            continue;
        }
        if tokens[0].eq_ignore_ascii_case("table") {
            parse_table(&mut ruleset, &tokens, line)?;
            continue;
        }
        if tokens[0].eq_ignore_ascii_case("nat") || tokens[0].eq_ignore_ascii_case("rdr") {
            ruleset.nat.push(parse_nat(&tokens, line)?);
            continue;
        }
        for expanded in expand_tables(&tokens, &ruleset.tables, line)? {
            if ruleset.rules.len() >= MAX_RULES {
                return Err(ParseError::new(line, "too many rules (limit 4096)"));
            }
            let refs: Vec<&str> = expanded.iter().map(String::as_str).collect();
            let rule = parse_rule(&refs, line)?;
            ruleset.rules.push(rule);
        }
    }
    Ok(ruleset)
}

fn parse_table(ruleset: &mut Ruleset, tokens: &[&str], line: usize) -> Result<(), ParseError> {
    if tokens.len() < 5 || tokens.get(2) != Some(&"{") || tokens.last() != Some(&"}") {
        return Err(ParseError::new(
            line,
            "expected `table <name> { <address>, ... }`",
        ));
    }
    let name = tokens[1]
        .strip_prefix('<')
        .and_then(|value| value.strip_suffix('>'))
        .ok_or_else(|| ParseError::new(line, "table name must be `<name>`"))?;
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(ParseError::new(line, "invalid table name"));
    }
    if ruleset.tables.iter().any(|table| table.name == name) {
        return Err(ParseError::new(line, "duplicate table"));
    }
    let mut addresses = Vec::new();
    for token in tokens.get(3..tokens.len().saturating_sub(1)).unwrap_or(&[]) {
        let text = token.trim_end_matches(',');
        if text.is_empty() {
            continue;
        }
        AddrSpec::parse(text)
            .map_err(|error| ParseError::new(line, format!("table address: {error}")))?;
        addresses.push(text.to_string());
    }
    if addresses.is_empty() {
        return Err(ParseError::new(line, "empty table"));
    }
    ruleset.tables.push(Table {
        name: name.to_string(),
        addresses,
    });
    Ok(())
}

fn expand_tables(
    tokens: &[&str],
    tables: &[Table],
    line: usize,
) -> Result<Vec<Vec<String>>, ParseError> {
    if let Some(index) = tokens
        .iter()
        .position(|token| token.starts_with('<') && token.ends_with('>'))
    {
        let name = tokens[index]
            .strip_prefix('<')
            .and_then(|value| value.strip_suffix('>'))
            .ok_or_else(|| ParseError::new(line, "malformed table reference"))?;
        let table = tables
            .iter()
            .find(|table| table.name == name)
            .ok_or_else(|| ParseError::new(line, format!("unknown table `<{name}>`")))?;
        let mut expanded = Vec::new();
        for address in &table.addresses {
            let mut replaced: Vec<String> =
                tokens.iter().map(|token| (*token).to_string()).collect();
            if let Some(slot) = replaced.get_mut(index) {
                *slot = address.clone();
            }
            let refs: Vec<&str> = replaced.iter().map(String::as_str).collect();
            expanded.extend(expand_tables(&refs, tables, line)?);
        }
        return Ok(expanded);
    }
    Ok(vec![
        tokens.iter().map(|token| (*token).to_string()).collect(),
    ])
}

fn parse_nat(tokens: &[&str], line: usize) -> Result<NatRule, ParseError> {
    let redirect = tokens[0].eq_ignore_ascii_case("rdr");
    let mut rule = NatRule {
        redirect,
        interface: IfaceSpec::Any,
        protocol: Protocol::Any,
        source: AddrPort::any(),
        destination: AddrPort::any(),
        address: AddrSpec::Any,
        port: None,
    };
    let mut index = 1;
    let mut saw_source = false;
    let mut saw_destination = false;
    let mut saw_arrow = false;
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
            let (source, used) = parse_addr_port(tokens, index + 1, line)?;
            rule.source = source;
            index += 1 + used;
        } else if token.eq_ignore_ascii_case("to") {
            if saw_destination {
                return Err(ParseError::new(line, "duplicate `to` clause"));
            }
            saw_destination = true;
            let (destination, used) = parse_addr_port(tokens, index + 1, line)?;
            rule.destination = destination;
            index += 1 + used;
        } else if token == "->" {
            if saw_arrow {
                return Err(ParseError::new(line, "duplicate `->`"));
            }
            saw_arrow = true;
            let address_text = tokens
                .get(index + 1)
                .ok_or_else(|| ParseError::new(line, "`->` needs an address"))?;
            rule.address = AddrSpec::parse(address_text)
                .map_err(|error| ParseError::new(line, format!("translation: {error}")))?;
            index += 2;
            if tokens
                .get(index)
                .is_some_and(|next| next.eq_ignore_ascii_case("port"))
            {
                let port_text = tokens
                    .get(index + 1)
                    .ok_or_else(|| ParseError::new(line, "`port` needs a value"))?;
                rule.port = Some(
                    port_text
                        .parse()
                        .map_err(|_| ParseError::new(line, "invalid translation port"))?,
                );
                index += 2;
            }
        } else {
            return Err(ParseError::new(
                line,
                format!("unknown NAT keyword `{token}`"),
            ));
        }
    }
    if !saw_arrow {
        return Err(ParseError::new(line, "NAT rule needs `-> <address>`"));
    }
    Ok(rule)
}

fn parse_set(ruleset: &mut Ruleset, tokens: &[&str], line: usize) -> Result<(), ParseError> {
    match tokens {
        [_, key, value] if key.eq_ignore_ascii_case("default") => {
            ruleset.default_action = parse_action(value).ok_or_else(|| {
                ParseError::new(line, format!("unknown default action `{value}`"))
            })?;
            Ok(())
        }
        [_, key, value] if key.eq_ignore_ascii_case("timeout") => {
            let _ = value;
            Err(ParseError::new(
                line,
                "timeouts are configured through the engine API, not the ruleset",
            ))
        }
        _ => Err(ParseError::new(line, "expected `set default pass|block`")),
    }
}

fn parse_rule(tokens: &[&str], line: usize) -> Result<Rule, ParseError> {
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
            let (source, used) = parse_addr_port(tokens, index + 1, line)?;
            rule.source = source;
            index += 1 + used;
        } else if token.eq_ignore_ascii_case("to") {
            if saw_destination {
                return Err(ParseError::new(line, "duplicate `to` clause"));
            }
            saw_destination = true;
            let (destination, used) = parse_addr_port(tokens, index + 1, line)?;
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
    Ok(rule)
}

fn set_direction(rule: &mut Rule, direction: Direction, line: usize) -> Result<(), ParseError> {
    if rule.direction.is_some() {
        return Err(ParseError::new(line, "duplicate direction"));
    }
    rule.direction = Some(direction);
    Ok(())
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
) -> Result<(AddrPort, usize), ParseError> {
    let address_text = tokens
        .get(start)
        .ok_or_else(|| ParseError::new(line, "expected an address"))?;
    let address = AddrSpec::parse(address_text)
        .map_err(|error| ParseError::new(line, format!("address: {error}")))?;
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

#[cfg(test)]
#[path = "parser_ut.rs"]
mod tests;

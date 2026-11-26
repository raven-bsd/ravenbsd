// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The pf.conf AST.
//!
//! The shapes follow the FreeBSD donor `struct pfctl_rule` use sites closely enough
//! that [`crate::print`] can reproduce `pfctl -nv` output, while staying a
//! first-party Rust model of the supported pf.conf subset.

use alloc::string::String;
use alloc::vec::Vec;

/// What a filter rule does with a matching packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Pass the packet.
    Pass,
    /// Drop the packet, optionally answering with `drop` or `return`.
    Block(Policy),
    /// Continue matching after a match (pf `match`).
    Match,
    /// Scrub the packet.
    Scrub,
    /// Do not scrub the packet.
    NoScrub,
}

/// Block-rule response policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Policy {
    /// Drop silently (the pf default).
    Drop,
    /// Send a TCP RST / ICMP unreachable.
    Return,
}

/// Traffic direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Inbound.
    In,
    /// Outbound.
    Out,
}

/// Address family selector (`inet` / `inet6`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddressFamily {
    /// IPv4.
    Inet,
    /// IPv6.
    Inet6,
}

/// Transport protocol selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    /// TCP.
    Tcp,
    /// UDP.
    Udp,
    /// ICMP (IPv4).
    Icmp,
    /// ICMPv6.
    Icmp6,
    /// A numeric IP protocol.
    Number(u8),
}

impl Protocol {
    /// The canonical name, or the number.
    #[must_use]
    pub fn name(self) -> Option<&'static str> {
        match self {
            Self::Tcp => Some("tcp"),
            Self::Udp => Some("udp"),
            Self::Icmp => Some("icmp"),
            Self::Icmp6 => Some("icmp6"),
            Self::Number(_) => None,
        }
    }
}

/// One address operand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostAddr {
    /// `any`.
    Any,
    /// A validated IPv4/IPv6 literal, optionally with a prefix length.
    Literal(String),
    /// A table reference (`<name>`).
    Table(String),
}

/// One port comparison.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PortSpec {
    /// `= n` (also the bare `port n` form).
    Eq(u16),
    /// `!= n`.
    Ne(u16),
    /// `< n`.
    Lt(u16),
    /// `<= n`.
    Le(u16),
    /// `> n`.
    Gt(u16),
    /// `>= n`.
    Ge(u16),
    /// `a:b`.
    Range(u16, u16),
    /// `{ a, b }`.
    Set(Vec<PortSpec>),
}

/// One entry of a source/destination specification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddrItem {
    /// True for a negated (`!`) entry.
    pub negative: bool,
    /// The address operand.
    pub address: HostAddr,
    /// The optional port condition.
    pub port: Option<PortSpec>,
}

/// A source or destination specification; an empty list means `any`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AddrPortSpec {
    /// Entries in file order.
    pub items: Vec<AddrItem>,
}

impl AddrPortSpec {
    /// True when the specification is the implicit `any`.
    #[must_use]
    pub fn is_any(&self) -> bool {
        self.items.is_empty()
    }
}

/// An interface reference with its optional `!` negation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IfaceRef {
    /// True for `on ! iface`.
    pub negative: bool,
    /// The interface name.
    pub name: String,
}

/// A TCP flag selector (`flags <set>/<mask>`), letters from `FSRPAUEC`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlagsSpec {
    /// Flags that must be set.
    pub set: String,
    /// Flags that are inspected.
    pub mask: String,
}

/// TCP state tracking for a rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateMode {
    /// No explicit state clause (the pf default for `pass`: keep state).
    Default,
    /// `keep state`.
    Keep,
    /// `modulate state`.
    Modulate,
    /// `synproxy state`.
    Synproxy,
    /// `no state`.
    None,
}

/// One filter rule (`pass`, `block`, `match`, `scrub`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    /// Rule action.
    pub action: Action,
    /// Direction condition.
    pub direction: Option<Direction>,
    /// `log`.
    pub log: bool,
    /// `quick`.
    pub quick: bool,
    /// `on <iface>`.
    pub interface: Option<IfaceRef>,
    /// `inet` / `inet6`.
    pub af: Option<AddressFamily>,
    /// `proto <p>`.
    pub proto: Option<Protocol>,
    /// `from ...`.
    pub source: AddrPortSpec,
    /// `to ...`.
    pub destination: AddrPortSpec,
    /// Explicit `flags`.
    pub flags: Option<FlagsSpec>,
    /// State tracking clause.
    pub state: StateMode,
}

/// NAT rule kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NatKind {
    /// `nat`.
    Nat,
    /// `rdr`.
    Rdr,
    /// `binat`.
    Binat,
    /// `no nat`.
    NoNat,
    /// `no rdr`.
    NoRdr,
    /// `no binat`.
    NoBinat,
}

impl NatKind {
    /// The canonical keyword.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Nat => "nat",
            Self::Rdr => "rdr",
            Self::Binat => "binat",
            Self::NoNat => "no nat",
            Self::NoRdr => "no rdr",
            Self::NoBinat => "no binat",
        }
    }
}

/// Redirect pool selection (`-> ... round-robin|source-hash|bitmask`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoolOption {
    /// `round-robin`.
    RoundRobin,
    /// `source-hash`.
    SourceHash,
    /// `bitmask`.
    Bitmask,
}

/// The `-> address [port p] [pool]` translation target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Redirect {
    /// Translation address.
    pub address: HostAddr,
    /// Optional translated port.
    pub port: Option<u16>,
    /// Optional pool selection.
    pub pool: Option<PoolOption>,
}

/// One NAT/redirect/binat rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NatRule {
    /// Rule kind.
    pub kind: NatKind,
    /// `pass` (NAT pass mode).
    pub pass: bool,
    /// `log`.
    pub log: bool,
    /// `quick`.
    pub quick: bool,
    /// `on <iface>`.
    pub interface: Option<IfaceRef>,
    /// `inet` / `inet6`.
    pub af: Option<AddressFamily>,
    /// `proto <p>`.
    pub proto: Option<Protocol>,
    /// `from ...`.
    pub source: AddrPortSpec,
    /// `to ...`.
    pub destination: AddrPortSpec,
    /// Translation target; required except for `no nat` / `no rdr` / `no binat`.
    pub redirect: Option<Redirect>,
}

/// Table definition flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableFlag {
    /// `const`.
    Const,
    /// `persist`.
    Persist,
    /// `counters`.
    Counters,
}

/// One table content entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TableEntry {
    /// An inline address, optionally negated.
    Address {
        /// True for a negated (`!`) entry.
        negative: bool,
        /// The address operand.
        address: HostAddr,
    },
    /// `file "path"`.
    File(String),
}

/// A table definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableDef {
    /// Table name (without the angle brackets).
    pub name: String,
    /// Declaration flags, in declaration order.
    pub flags: Vec<TableFlag>,
    /// Inline entries / file references.
    pub entries: Vec<TableEntry>,
}

/// A policy option value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyValue {
    /// `drop`.
    Drop,
    /// `return`.
    Return,
}

/// A `set state-policy` value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatePolicyValue {
    /// `floating`.
    Floating,
    /// `if-bound`.
    IfBound,
}

/// One parsed `set` directive of the supported subset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OptionDirective {
    /// `set block-policy drop|return`.
    BlockPolicy(PolicyValue),
    /// `set fail-policy drop|return`.
    FailPolicy(PolicyValue),
    /// `set require-order yes|no`.
    RequireOrder(bool),
    /// `set state-policy floating|if-bound`.
    StatePolicy(StatePolicyValue),
    /// `set skip on <iface>`.
    Skip(IfaceRef),
    /// `set optimization none|basic|profile`.
    Optimization(String),
    /// `set loginterface <iface>`.
    LogInterface(String),
    /// `set timeout <name> <value>` (or a braced list).
    Timeout(Vec<(String, u32)>),
    /// `set limit <name> <value>` (or a braced list).
    Limit(Vec<(String, u32)>),
    /// `set hostid <n>`.
    HostId(u32),
}

/// One top-level statement in file order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Statement {
    /// `<name> = "<value>"`.
    Macro {
        /// Macro name.
        name: String,
        /// Macro value.
        value: String,
    },
    /// `table <name> ...`.
    Table(TableDef),
    /// `set ...`.
    Option(OptionDirective),
    /// A filter rule.
    Rule(Rule),
    /// A NAT/redirect rule.
    Nat(NatRule),
    /// `anchor "name"`.
    Anchor(String),
    /// `include "path"` (retained when the caller does not expand includes).
    Include(String),
}

/// A parsed pf.conf.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Config {
    /// Statements in file order.
    pub statements: Vec<Statement>,
}

impl Config {
    /// An empty configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// True when `text` is a valid dotted-quad IPv4 address.
#[must_use]
pub fn is_ipv4(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    parts.iter().all(|part| {
        !part.is_empty()
            && part.len() <= 3
            && part.chars().all(|ch| ch.is_ascii_digit())
            && (part.len() == 1 || !part.starts_with('0'))
            && part.parse::<u16>().is_ok_and(|value| value <= 255)
    })
}

/// True when `text` is a valid IPv6 address (with at most one `::`).
#[must_use]
pub fn is_ipv6(text: &str) -> bool {
    if text.is_empty() || !text.contains(':') {
        return false;
    }
    let (head, tail) = match text.split_once("::") {
        Some((head, tail)) => {
            if tail.contains("::") {
                return false;
            }
            (head, Some(tail))
        }
        None => (text, None),
    };
    let Some(head_count) = group_count(head) else {
        return false;
    };
    match tail {
        Some(tail) => match group_count(tail) {
            Some(tail_count) => head_count + tail_count < 8,
            None => false,
        },
        None => head_count == 8,
    }
}

/// Count 1-4 hex groups; `None` when any group is malformed.
fn group_count(text: &str) -> Option<usize> {
    if text.is_empty() {
        return Some(0);
    }
    let mut count = 0;
    for group in text.split(':') {
        if group.is_empty() || group.len() > 4 || !group.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return None;
        }
        count += 1;
    }
    Some(count)
}

/// True when `text` is an address literal or an address with a prefix length.
#[must_use]
pub fn is_address_literal(text: &str) -> bool {
    match text.split_once('/') {
        None => is_ipv4(text) || is_ipv6(text),
        Some((address, prefix)) => {
            if prefix.is_empty()
                || prefix.len() > 3
                || !prefix.chars().all(|ch| ch.is_ascii_digit())
            {
                return false;
            }
            let Ok(prefix) = prefix.parse::<u16>() else {
                return false;
            };
            if is_ipv4(address) {
                prefix <= 32
            } else if is_ipv6(address) {
                prefix <= 128
            } else {
                false
            }
        }
    }
}

#[cfg(test)]
#[path = "ast_ut.rs"]
mod tests;

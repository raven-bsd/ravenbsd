// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Ruleset AST and per-rule matching.

use alloc::string::String;
use alloc::vec::Vec;

use crate::address::{AddrSpec, Ipv4Addr, Ipv6Addr, PortSpec};

/// Traffic direction relative to the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Packets arriving from the network.
    In,
    /// Packets leaving the host.
    Out,
}

/// Transport protocol a rule applies to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    /// Every protocol.
    Any,
    /// TCP.
    Tcp,
    /// UDP.
    Udp,
    /// ICMP.
    Icmp,
}

/// What the filter does with a matching packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Allow the packet.
    Pass,
    /// Drop the packet.
    Block,
}

/// Whether a passing rule keeps a connection state entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatePolicy {
    /// Track the connection (the default for `pass`).
    Keep,
    /// Evaluate every packet against the rules.
    None,
}

/// The interface condition of a rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IfaceSpec {
    /// Every interface.
    Any,
    /// One interface name.
    Name(String),
}

impl IfaceSpec {
    /// True when `interface` satisfies this condition.
    #[must_use]
    pub fn matches(&self, interface: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Name(name) => name == interface,
        }
    }
}

/// TCP control bits used by the state table.
pub mod tcp_flag {
    /// FIN.
    pub const FIN: u8 = 0x01;
    /// SYN.
    pub const SYN: u8 = 0x02;
    /// RST.
    pub const RST: u8 = 0x04;
    /// ACK.
    pub const ACK: u8 = 0x10;
}

/// One packet handed to the filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Packet<'a> {
    /// Direction of the packet.
    pub direction: Direction,
    /// Interface the packet arrived on or leaves through.
    pub interface: &'a str,
    /// Transport protocol.
    pub protocol: Protocol,
    /// Source address.
    pub source: Ipv4Addr,
    /// Destination address.
    pub destination: Ipv4Addr,
    /// Source port (zero for ICMP).
    pub source_port: u16,
    /// Destination port (zero for ICMP).
    pub destination_port: u16,
    /// IP payload length used for byte counters.
    pub length: u32,
    /// TCP control bits (zero for other protocols).
    pub tcp_flags: u8,
}

impl<'a> Packet<'a> {
    /// Build a TCP packet.
    #[must_use]
    #[allow(clippy::too_many_arguments)] // the packet fields are the API
    pub fn tcp(
        direction: Direction,
        interface: &'a str,
        source: Ipv4Addr,
        source_port: u16,
        destination: Ipv4Addr,
        destination_port: u16,
        tcp_flags: u8,
        length: u32,
    ) -> Self {
        Self {
            direction,
            interface,
            protocol: Protocol::Tcp,
            source,
            destination,
            source_port,
            destination_port,
            length,
            tcp_flags,
        }
    }

    /// Build a UDP packet.
    #[must_use]
    pub fn udp(
        direction: Direction,
        interface: &'a str,
        source: Ipv4Addr,
        source_port: u16,
        destination: Ipv4Addr,
        destination_port: u16,
        length: u32,
    ) -> Self {
        Self {
            direction,
            interface,
            protocol: Protocol::Udp,
            source,
            destination,
            source_port,
            destination_port,
            length,
            tcp_flags: 0,
        }
    }

    /// Build an ICMP packet.
    #[must_use]
    pub fn icmp(
        direction: Direction,
        interface: &'a str,
        source: Ipv4Addr,
        destination: Ipv4Addr,
        length: u32,
    ) -> Self {
        Self {
            direction,
            interface,
            protocol: Protocol::Icmp,
            source,
            destination,
            source_port: 0,
            destination_port: 0,
            length,
            tcp_flags: 0,
        }
    }
}

/// One IPv6 packet handed to the stateless filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Packet6<'a> {
    /// Direction of the packet.
    pub direction: Direction,
    /// Interface the packet arrived on or leaves through.
    pub interface: &'a str,
    /// Transport protocol.
    pub protocol: Protocol,
    /// Source address.
    pub source: Ipv6Addr,
    /// Destination address.
    pub destination: Ipv6Addr,
    /// Source port (zero for ICMPv6).
    pub source_port: u16,
    /// Destination port (zero for ICMPv6).
    pub destination_port: u16,
    /// IP payload length used for byte counters.
    pub length: u32,
    /// TCP control bits (zero for other protocols).
    pub tcp_flags: u8,
}

/// One address-plus-port condition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AddrPort {
    /// Address condition.
    pub address: AddrSpec,
    /// Port condition.
    pub port: PortSpec,
}

impl AddrPort {
    /// Every address and port.
    #[must_use]
    pub const fn any() -> Self {
        Self {
            address: AddrSpec::Any,
            port: PortSpec::Any,
        }
    }

    fn matches(&self, address: Ipv4Addr, port: u16, has_ports: bool) -> bool {
        if !self.address.matches(address) {
            return false;
        }
        if has_ports {
            self.port.matches(port)
        } else {
            self.port == PortSpec::Any
        }
    }

    fn matches_v6(&self, address: Ipv6Addr, port: u16, has_ports: bool) -> bool {
        if !self.address.matches_v6(address) {
            return false;
        }
        if has_ports {
            self.port.matches(port)
        } else {
            self.port == PortSpec::Any
        }
    }
}

/// One pf-like filter rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    /// Pass or block.
    pub action: Action,
    /// Stop at this rule instead of using the last match.
    pub quick: bool,
    /// Direction condition; `None` matches both directions.
    pub direction: Option<Direction>,
    /// Interface condition.
    pub interface: IfaceSpec,
    /// Protocol condition.
    pub protocol: Protocol,
    /// Source condition.
    pub source: AddrPort,
    /// Destination condition.
    pub destination: AddrPort,
    /// State tracking policy.
    pub state: StatePolicy,
    /// Whether the rule requests logging.
    pub log: bool,
}

impl Rule {
    /// True when this rule applies to `packet`.
    #[must_use]
    pub fn matches(&self, packet: &Packet<'_>) -> bool {
        if let Some(direction) = self.direction
            && direction != packet.direction
        {
            return false;
        }
        if !self.interface.matches(packet.interface) {
            return false;
        }
        if self.protocol != Protocol::Any && self.protocol != packet.protocol {
            return false;
        }
        let has_ports = matches!(packet.protocol, Protocol::Tcp | Protocol::Udp);
        self.source
            .matches(packet.source, packet.source_port, has_ports)
            && self
                .destination
                .matches(packet.destination, packet.destination_port, has_ports)
    }

    /// True when this rule applies to an IPv6 packet.
    #[must_use]
    pub fn matches_v6(&self, packet: &Packet6<'_>) -> bool {
        if let Some(direction) = self.direction
            && direction != packet.direction
        {
            return false;
        }
        if !self.interface.matches(packet.interface) {
            return false;
        }
        if self.protocol != Protocol::Any && self.protocol != packet.protocol {
            return false;
        }
        let has_ports = matches!(packet.protocol, Protocol::Tcp | Protocol::Udp);
        self.source
            .matches_v6(packet.source, packet.source_port, has_ports)
            && self
                .destination
                .matches_v6(packet.destination, packet.destination_port, has_ports)
    }
}

/// One pf table with its addresses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    /// Table name without angle brackets.
    pub name: String,
    /// Addresses in declaration order, as their original rule text.
    pub addresses: Vec<String>,
}

/// A NAT or redirect rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NatRule {
    /// NAT rewrites the source; redirect rewrites the destination.
    pub redirect: bool,
    /// Interface condition.
    pub interface: IfaceSpec,
    /// Protocol condition.
    pub protocol: Protocol,
    /// Source condition.
    pub source: AddrPort,
    /// Destination condition.
    pub destination: AddrPort,
    /// Translated address.
    pub address: AddrSpec,
    /// Translated port, when the rule names one.
    pub port: Option<u16>,
}

/// One applied translation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Translation {
    /// Translated address.
    pub address: AddrSpec,
    /// Translated port, or zero when unchanged.
    pub port: u16,
    /// True when the destination was rewritten (redirect).
    pub destination: bool,
}

impl NatRule {
    /// True when this rule matches the packet.
    #[must_use]
    pub fn matches(&self, packet: &Packet<'_>) -> bool {
        let wants = if self.redirect {
            Direction::In
        } else {
            Direction::Out
        };
        if packet.direction != wants
            || !self.interface.matches(packet.interface)
            || (self.protocol != Protocol::Any && self.protocol != packet.protocol)
        {
            return false;
        }
        let has_ports = matches!(packet.protocol, Protocol::Tcp | Protocol::Udp);
        self.source
            .matches(packet.source, packet.source_port, has_ports)
            && self
                .destination
                .matches(packet.destination, packet.destination_port, has_ports)
    }

    /// The translation for a matching packet.
    #[must_use]
    pub fn translation(&self, packet: &Packet<'_>) -> Option<Translation> {
        if !self.matches(packet) {
            return None;
        }
        Some(Translation {
            address: self.address,
            port: self.port.unwrap_or(if self.redirect {
                packet.destination_port
            } else {
                packet.source_port
            }),
            destination: self.redirect,
        })
    }
}

/// A parsed ruleset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ruleset {
    /// Rules in file order.
    pub rules: Vec<Rule>,
    /// Verdict when no rule matches.
    pub default_action: Action,
    /// Named tables.
    pub tables: Vec<Table>,
    /// NAT and redirect rules.
    pub nat: Vec<NatRule>,
}

impl Ruleset {
    /// An empty ruleset with the pf default: block.
    #[must_use]
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            default_action: Action::Block,
            tables: Vec::new(),
            nat: Vec::new(),
        }
    }
}

impl Default for Ruleset {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "ast_ut.rs"]
mod tests;

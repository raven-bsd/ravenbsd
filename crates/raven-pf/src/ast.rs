// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Ruleset AST and per-rule matching.
//!
//! Donor: `struct pf_rule` / `struct pf_rule_addr` in `sys/netpfil/pf/pf.h` and the
//! attribute chain in `pf_match_rule()` (`sys/netpfil/pf/pf.c:5959`). The attribute
//! order here mirrors the donor: direction, interface, address family, protocol,
//! source, destination, then per-protocol ports. TCP flag-set conditions are part of
//! the 0.1.0 state machine, not of rule matching.

use alloc::string::String;
use alloc::vec::Vec;

use crate::address::{AddrSpec, IpAddr, PortSpec};
use crate::nat::NatTable;
use crate::table::Table;

/// Traffic direction relative to the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Packets arriving from the network.
    In,
    /// Packets leaving the host.
    Out,
}

/// Address family of a rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// IPv4.
    Inet,
    /// IPv6.
    Inet6,
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
    /// ICMP for IPv4.
    Icmp,
    /// ICMPv6.
    Icmpv6,
}

impl Protocol {
    /// True when the protocol carries ports.
    #[must_use]
    pub const fn has_ports(self) -> bool {
        matches!(self, Self::Tcp | Self::Udp)
    }
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
    /// PSH.
    pub const PSH: u8 = 0x08;
    /// ACK.
    pub const ACK: u8 = 0x10;
    /// URG.
    pub const URG: u8 = 0x20;
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
    pub source: IpAddr,
    /// Destination address.
    pub destination: IpAddr,
    /// Source port (zero for ICMP).
    pub source_port: u16,
    /// Destination port (zero for ICMP).
    pub destination_port: u16,
    /// IP payload length used for byte counters.
    pub length: u32,
    /// TCP control bits (zero for other protocols).
    pub tcp_flags: u8,
    /// An IP fragment. Donor `pf_match_rule()` skips the port conditions for fragments.
    pub fragment: bool,
}

impl<'a> Packet<'a> {
    /// Build a TCP packet.
    #[must_use]
    #[allow(clippy::too_many_arguments)] // the packet fields are the API
    pub fn tcp(
        direction: Direction,
        interface: &'a str,
        source: impl Into<IpAddr>,
        source_port: u16,
        destination: impl Into<IpAddr>,
        destination_port: u16,
        tcp_flags: u8,
        length: u32,
    ) -> Self {
        Self {
            direction,
            interface,
            protocol: Protocol::Tcp,
            source: source.into(),
            destination: destination.into(),
            source_port,
            destination_port,
            length,
            tcp_flags,
            fragment: false,
        }
    }

    /// Build a UDP packet.
    #[must_use]
    pub fn udp(
        direction: Direction,
        interface: &'a str,
        source: impl Into<IpAddr>,
        source_port: u16,
        destination: impl Into<IpAddr>,
        destination_port: u16,
        length: u32,
    ) -> Self {
        Self {
            direction,
            interface,
            protocol: Protocol::Udp,
            source: source.into(),
            destination: destination.into(),
            source_port,
            destination_port,
            length,
            tcp_flags: 0,
            fragment: false,
        }
    }

    /// Build an ICMP packet.
    #[must_use]
    pub fn icmp(
        direction: Direction,
        interface: &'a str,
        source: impl Into<IpAddr>,
        destination: impl Into<IpAddr>,
        length: u32,
    ) -> Self {
        Self {
            direction,
            interface,
            protocol: Protocol::Icmp,
            source: source.into(),
            destination: destination.into(),
            source_port: 0,
            destination_port: 0,
            length,
            tcp_flags: 0,
            fragment: false,
        }
    }

    /// Build an ICMPv6 packet.
    #[must_use]
    pub fn icmpv6(
        direction: Direction,
        interface: &'a str,
        source: impl Into<IpAddr>,
        destination: impl Into<IpAddr>,
        length: u32,
    ) -> Self {
        Self {
            direction,
            interface,
            protocol: Protocol::Icmpv6,
            source: source.into(),
            destination: destination.into(),
            source_port: 0,
            destination_port: 0,
            length,
            tcp_flags: 0,
            fragment: false,
        }
    }

    /// Mark the packet as an IP fragment.
    #[must_use]
    pub fn fragmented(mut self) -> Self {
        self.fragment = true;
        self
    }

    /// The address family of the packet.
    #[must_use]
    pub fn family(&self) -> Family {
        if self.source.is_v4() {
            Family::Inet
        } else {
            Family::Inet6
        }
    }
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

    pub(crate) fn matches(
        &self,
        address: IpAddr,
        port: u16,
        has_ports: bool,
        tables: &[Table],
    ) -> bool {
        if !self.address.matches(address, tables) {
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
    /// Address family condition; `None` matches both.
    pub family: Option<Family>,
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
    pub fn matches(&self, packet: &Packet<'_>, tables: &[Table]) -> bool {
        if let Some(direction) = self.direction
            && direction != packet.direction
        {
            return false;
        }
        if !self.interface.matches(packet.interface) {
            return false;
        }
        if let Some(family) = self.family
            && family != packet.family()
        {
            return false;
        }
        if self.protocol != Protocol::Any && self.protocol != packet.protocol {
            return false;
        }
        if packet.fragment {
            // Donor `pf_match_rule()` (`PF_VPROTO_FRAGMENT`) fails a rule that names a
            // source or destination port: `PF_TEST_ATTRIB((r->src.port_op ||
            // r->dst.port_op), fail)`. A fragment can still match an address-only rule;
            // TCP flags and other per-protocol attributes are likewise not evaluated.
            if self.source.port != PortSpec::Any || self.destination.port != PortSpec::Any {
                return false;
            }
            return self.source.address.matches(packet.source, tables)
                && self.destination.address.matches(packet.destination, tables);
        }
        let has_ports = packet.protocol.has_ports();
        self.source
            .matches(packet.source, packet.source_port, has_ports, tables)
            && self.destination.matches(
                packet.destination,
                packet.destination_port,
                has_ports,
                tables,
            )
    }
}

/// A parsed ruleset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ruleset {
    /// Rules in file order.
    pub rules: Vec<Rule>,
    /// Verdict when no rule matches.
    pub default_action: Action,
    /// Named address tables.
    pub tables: Vec<Table>,
    /// NAT and redirect rules.
    pub nat: NatTable,
}

impl Ruleset {
    /// An empty ruleset with the project default: block.
    #[must_use]
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            default_action: Action::Block,
            tables: Vec::new(),
            nat: NatTable::new(),
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

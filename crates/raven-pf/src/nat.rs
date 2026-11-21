// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Portions ported from FreeBSD sys/netpfil/pf/pf_lb.c (BSD-2-Clause):
// Copyright (c) 2001 Daniel Hartmeier, Copyright (c) 2002-2008 Henning Brauer.
// All rights reserved. Full donor notice and revision: PROVENANCE.toml, docs/pf.md section 13.

//! NAT and redirect (RDR) rule evaluation.
//!
//! Donor: `sys/netpfil/pf/pf_lb.c` — `pf_match_translation()` / `pf_match_translation_rule()`
//! for rule selection, `pf_map_addr()` for pool address selection, `pf_get_sport()` for the
//! source-port search, and `pf_get_transaddr()` for the redirect destination-port formula:
//!
//! ```text
//! div       = proxy_port[1] - proxy_port[0] + 1        (0 -> 1)
//! tmp_nport = ((ndport - r->dst.port[0]) % div) + proxy_port[0]
//! if tmp_nport > 65535 { tmp_nport -= 65535 }
//! ```
//!
//! The pool kinds are the donor `PF_POOL_*` values. Address selection is documented in
//! `docs/pf.md`; `bitmask` over a multi-address pool is approximated by the first address
//! (deviation D-pf-pool-bitmask) because the 0.1.0 parser has no pool-mask option.

use alloc::vec::Vec;

use crate::address::IpAddr;
use crate::ast::{AddrPort, Direction, IfaceSpec, Packet, Protocol};
use crate::table::Table;
/// Largest number of NAT/RDR rules in one ruleset.
pub const MAX_NAT_RULES: usize = 1024;
/// Largest number of addresses in one pool.
pub const MAX_POOL_ADDRESSES: usize = 16;
/// Donor `PF_NAT_PROXY_PORT_LOW`: first port of the default NAT port range.
pub const NAT_PROXY_PORT_LOW: u16 = 50001;
/// Donor `PF_NAT_PROXY_PORT_HIGH`: last port of the default NAT port range.
pub const NAT_PROXY_PORT_HIGH: u16 = 65535;

/// Donor `PF_POOL_*` selection strategy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoolKind {
    /// `PF_POOL_NONE`: the first configured address.
    None,
    /// `PF_POOL_BITMASK`: the address selected by the pool mask.
    Bitmask,
    /// `PF_POOL_RANDOM`: a pseudo-random pool address.
    Random,
    /// `PF_POOL_SRCHASH`: an address hashed from the source address.
    SourceHash,
    /// `PF_POOL_ROUNDROBIN`: pool addresses in turn.
    RoundRobin,
}

/// One translation pool.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pool {
    /// Address selection strategy.
    pub kind: PoolKind,
    /// Pool addresses, in configuration order.
    pub addresses: Vec<IpAddr>,
    /// Explicit port range (`port N` or `port A:B`); `None` means the action default.
    pub proxy_port: Option<(u16, u16)>,
    /// `static-port`: never rewrite the source port (NAT only).
    pub static_port: bool,
}

impl Pool {
    /// An empty pool with the donor default strategy.
    #[must_use]
    pub fn new() -> Self {
        Self {
            kind: PoolKind::None,
            addresses: Vec::new(),
            proxy_port: None,
            static_port: false,
        }
    }
}

impl Default for Pool {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether a rule translates the source (NAT) or the destination (RDR).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NatAction {
    /// Source NAT.
    Nat,
    /// Destination redirect.
    Rdr,
}

/// One NAT or redirect rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NatRule {
    /// NAT or redirect.
    pub action: NatAction,
    /// Direction condition; `nat` defaults to outbound and `rdr` to inbound.
    pub direction: Option<Direction>,
    /// Interface condition.
    pub interface: IfaceSpec,
    /// Protocol condition.
    pub protocol: Protocol,
    /// Source condition.
    pub source: AddrPort,
    /// Destination condition.
    pub destination: AddrPort,
    /// Translation pool.
    pub pool: Pool,
    /// Log matching packets.
    pub log: bool,
}

/// The fields a translation rewrites.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Translation {
    /// Index of the NAT rule that produced it.
    pub rule: usize,
    /// New source address.
    pub source: IpAddr,
    /// New source port.
    pub source_port: u16,
    /// New destination address.
    pub destination: IpAddr,
    /// New destination port.
    pub destination_port: u16,
}

/// Why a matching translation rule produced no translation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NatError {
    /// The pool has no address to translate to.
    NoAddress,
    /// No free port in the configured range.
    PoolExhausted,
}

/// The NAT/RDR rule table.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct NatTable {
    rules: Vec<NatRule>,
    cursor: u64,
    address_cursor: u64,
}

impl NatTable {
    /// An empty table.
    #[must_use]
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            cursor: 0,
            address_cursor: 0,
        }
    }

    /// Append one rule.
    ///
    /// # Errors
    ///
    /// Returns `false` when the table already holds [`MAX_NAT_RULES`] rules.
    pub fn push(&mut self, rule: NatRule) -> bool {
        if self.rules.len() >= MAX_NAT_RULES {
            return false;
        }
        self.rules.push(rule);
        true
    }

    /// The rules in configuration order.
    #[must_use]
    pub fn rules(&self) -> &[NatRule] {
        &self.rules
    }

    /// Number of rules.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// True when the table has no rule.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Find the first rule that applies to `packet` (donor: first match wins for translation).
    #[must_use]
    pub fn find(&self, packet: &Packet<'_>, tables: &[Table]) -> Option<usize> {
        self.rules
            .iter()
            .position(|rule| rule.matches(packet, tables))
    }

    /// Translate `packet` with rule `index`.
    ///
    /// `in_use` reports whether a candidate source port is already owned by a live state;
    /// the donor `pf_get_sport()` conflict check is the `pf_find_state_all_exists()` call.
    ///
    /// # Errors
    ///
    /// Returns [`NatError`] when the rule matched but no translation is possible. The caller
    /// must drop the packet (fail-closed), like donor `pf_test_rule()` on a translation error.
    pub fn translate<F>(
        &mut self,
        index: usize,
        packet: &Packet<'_>,
        in_use: F,
    ) -> Result<Option<Translation>, NatError>
    where
        F: Fn(IpAddr, u16) -> bool,
    {
        let Some(rule) = self.rules.get(index) else {
            return Ok(None);
        };
        if rule.pool.addresses.is_empty() {
            return Err(NatError::NoAddress);
        }
        let Some(address) = select_address(&mut self.address_cursor, &rule.pool, packet) else {
            return Err(NatError::NoAddress);
        };
        let family_ok = match rule.action {
            NatAction::Nat => address.is_v4() == packet.source.is_v4(),
            NatAction::Rdr => address.is_v4() == packet.destination.is_v4(),
        };
        if !family_ok {
            return Err(NatError::NoAddress);
        }
        match rule.action {
            NatAction::Nat => {
                let source_port = if rule.pool.static_port
                    || !matches!(packet.protocol, Protocol::Tcp | Protocol::Udp)
                {
                    packet.source_port
                } else {
                    let (low, high) = rule
                        .pool
                        .proxy_port
                        .unwrap_or((NAT_PROXY_PORT_LOW, NAT_PROXY_PORT_HIGH));
                    allocate_port(low, high, address, &mut self.cursor, &in_use)?
                };
                Ok(Some(Translation {
                    rule: index,
                    source: address,
                    source_port,
                    destination: packet.destination,
                    destination_port: packet.destination_port,
                }))
            }
            NatAction::Rdr => {
                let destination_port = match (rule.pool.proxy_port, rule.destination.port.span()) {
                    (Some((low, high)), Some((base, _))) => {
                        remap_redirect_port(packet.destination_port, base, low, high)
                    }
                    (Some((low, _)), None) => low,
                    (None, _) => packet.destination_port,
                };
                Ok(Some(Translation {
                    rule: index,
                    source: packet.source,
                    source_port: packet.source_port,
                    destination: address,
                    destination_port,
                }))
            }
        }
    }
}

fn select_address(cursor: &mut u64, pool: &Pool, packet: &Packet<'_>) -> Option<IpAddr> {
    let len = pool.addresses.len();
    if len == 0 {
        return None;
    }
    let index = match pool.kind {
        PoolKind::None | PoolKind::Bitmask => 0,
        PoolKind::Random => (pseudo_random(packet) as usize) % len,
        PoolKind::SourceHash => (fnv1a(packet.source.low_u64()) as usize) % len,
        PoolKind::RoundRobin => {
            let index = (*cursor as usize) % len;
            *cursor = cursor.wrapping_add(1);
            index
        }
    };
    pool.addresses.get(index).copied()
}

impl NatRule {
    fn matches(&self, packet: &Packet<'_>, tables: &[Table]) -> bool {
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
        if packet.fragment {
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

/// The donor redirect destination-port formula (`pf_get_transaddr`, PF_RDR case).
///
/// Returns `dport` unchanged when `low > high`.
#[must_use]
pub fn remap_redirect_port(dport: u16, base: u16, low: u16, high: u16) -> u16 {
    if low > high {
        return dport;
    }
    let div = u32::from(high - low) + 1;
    let offset = u32::from(dport).wrapping_sub(u32::from(base)) % div;
    let total = offset + u32::from(low);
    if total > u32::from(u16::MAX) {
        u16::try_from(total - u32::from(u16::MAX)).unwrap_or(u16::MAX)
    } else {
        u16::try_from(total).unwrap_or(u16::MAX)
    }
}

fn allocate_port<F>(
    low: u16,
    high: u16,
    address: IpAddr,
    cursor: &mut u64,
    in_use: &F,
) -> Result<u16, NatError>
where
    F: Fn(IpAddr, u16) -> bool,
{
    if low > high {
        return Err(NatError::PoolExhausted);
    }
    let width = u32::from(high - low) + 1;
    let offset = u32::try_from(*cursor % u64::from(width)).unwrap_or(0);
    for step in 0..width {
        let candidate = u32::from(low) + ((offset + step) % width);
        let candidate = u16::try_from(candidate).unwrap_or(high);
        if !in_use(address, candidate) {
            *cursor = cursor.wrapping_add(1);
            return Ok(candidate);
        }
    }
    Err(NatError::PoolExhausted)
}

fn fnv1a(value: u64) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in value.to_be_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn pseudo_random(packet: &Packet<'_>) -> u64 {
    let mut value = packet.source.low_u64()
        ^ packet.destination.low_u64().rotate_left(17)
        ^ u64::from(packet.source_port).rotate_left(33)
        ^ u64::from(packet.destination_port).rotate_left(49);
    value ^= value << 13;
    value ^= value >> 7;
    value ^= value << 17;
    value
}

#[cfg(test)]
#[path = "nat_ut.rs"]
mod tests;

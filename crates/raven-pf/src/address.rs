// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! IPv4/IPv6 address and port specifications for pf-like rules.
//!
//! Donor: the `struct pf_addr` union and `struct pf_rule_addr` in
//! `sys/netpfil/pf/pf.h`, plus `pf_match_addr()` in `sys/netpfil/pf/pf.c`
//! (`ADDRMASK` and `RANGE` matching). Prefix matching is the `ADDRMASK` case;
//! `RANGE`, `NOROUTE`, `URPFFAILED` and `DYNIFTL` are deferred (see `docs/pf.md`).

use alloc::format;
use alloc::string::String;
use core::fmt;

use crate::table::{MATCH_DEPTH_LIMIT, Table};

/// One IPv4 address in network byte order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Ipv4Addr {
    octets: [u8; 4],
}

impl Ipv4Addr {
    /// Build an address from its four octets.
    #[must_use]
    pub const fn new(a: u8, b: u8, c: u8, d: u8) -> Self {
        Self {
            octets: [a, b, c, d],
        }
    }

    /// Build an address from a big-endian `u32`.
    #[must_use]
    pub const fn from_u32(value: u32) -> Self {
        Self {
            octets: value.to_be_bytes(),
        }
    }

    /// The four octets in network order.
    #[must_use]
    pub const fn octets(self) -> [u8; 4] {
        self.octets
    }

    /// The address as a big-endian `u32`.
    #[must_use]
    pub const fn to_u32(self) -> u32 {
        u32::from_be_bytes(self.octets)
    }

    /// Parse dotted-quad notation (`10.0.0.1`).
    ///
    /// # Errors
    ///
    /// Returns [`AddrError::BadAddress`] for a non-numeric octet, an octet above 255, a
    /// leading zero on a multi-digit octet or a part count other than four.
    pub fn parse(text: &str) -> Result<Self, AddrError> {
        let mut octets = [0u8; 4];
        let mut parts = text.split('.');
        for (index, slot) in octets.iter_mut().enumerate() {
            let part = parts.next().ok_or(AddrError::BadAddress)?;
            *slot = parse_octet(part).ok_or(AddrError::BadAddress)?;
            if index == 3 && parts.next().is_some() {
                return Err(AddrError::BadAddress);
            }
        }
        if parts.next().is_some() {
            return Err(AddrError::BadAddress);
        }
        Ok(Self { octets })
    }
}

fn parse_octet(text: &str) -> Option<u8> {
    if text.is_empty() || text.len() > 3 {
        return None;
    }
    if text.len() > 1 && text.starts_with('0') {
        return None;
    }
    let mut value = 0u16;
    for byte in text.bytes() {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value * 10 + u16::from(byte - b'0');
    }
    u8::try_from(value).ok()
}

impl fmt::Display for Ipv4Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b, c, d] = self.octets;
        write!(f, "{a}.{b}.{c}.{d}")
    }
}

/// One IPv6 address in network byte order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Ipv6Addr {
    octets: [u8; 16],
}

impl Ipv6Addr {
    /// Build an address from its sixteen octets.
    #[must_use]
    pub const fn new(octets: [u8; 16]) -> Self {
        Self { octets }
    }

    /// Build an address from eight 16-bit groups in network order.
    #[must_use]
    pub const fn from_segments(segments: [u16; 8]) -> Self {
        let [a, b, c, d, e, f, g, h] = segments;
        let [a0, a1] = a.to_be_bytes();
        let [b0, b1] = b.to_be_bytes();
        let [c0, c1] = c.to_be_bytes();
        let [d0, d1] = d.to_be_bytes();
        let [e0, e1] = e.to_be_bytes();
        let [f0, f1] = f.to_be_bytes();
        let [g0, g1] = g.to_be_bytes();
        let [h0, h1] = h.to_be_bytes();
        Self {
            octets: [
                a0, a1, b0, b1, c0, c1, d0, d1, e0, e1, f0, f1, g0, g1, h0, h1,
            ],
        }
    }

    /// The sixteen octets in network order.
    #[must_use]
    pub const fn octets(self) -> [u8; 16] {
        self.octets
    }

    /// The eight 16-bit groups in network order.
    #[must_use]
    pub const fn segments(self) -> [u16; 8] {
        let o = self.octets;
        [
            u16::from_be_bytes([o[0], o[1]]),
            u16::from_be_bytes([o[2], o[3]]),
            u16::from_be_bytes([o[4], o[5]]),
            u16::from_be_bytes([o[6], o[7]]),
            u16::from_be_bytes([o[8], o[9]]),
            u16::from_be_bytes([o[10], o[11]]),
            u16::from_be_bytes([o[12], o[13]]),
            u16::from_be_bytes([o[14], o[15]]),
        ]
    }

    /// Parse hexadecimal group notation with at most one `::` elision.
    ///
    /// Embedded IPv4 tails (`::ffff:10.0.0.1`) are not part of the 0.1.0 subset and are
    /// rejected; see `docs/pf.md`.
    ///
    /// # Errors
    ///
    /// Returns [`AddrError::BadAddress`] for a bad group, more than one `::`, a group count
    /// outside `1..=8` without elision or `8..` with elision.
    pub fn parse(text: &str) -> Result<Self, AddrError> {
        if text.is_empty() {
            return Err(AddrError::BadAddress);
        }
        let (left_text, right_text, elided) = match text.split_once("::") {
            Some((left, right)) => {
                if right.contains("::") {
                    return Err(AddrError::BadAddress);
                }
                (left, right, true)
            }
            None => (text, "", false),
        };
        let left = parse_v6_groups(left_text)?;
        let right = parse_v6_groups(right_text)?;
        let total = left.len() + right.len();
        let mut segments = [0u16; 8];
        if elided {
            if total > 7 {
                return Err(AddrError::BadAddress);
            }
            for (slot, value) in segments.iter_mut().zip(left.iter().copied()) {
                *slot = value;
            }
            let offset = 8 - right.len();
            for (slot, value) in segments.iter_mut().skip(offset).zip(right.iter().copied()) {
                *slot = value;
            }
        } else {
            if total != 8 {
                return Err(AddrError::BadAddress);
            }
            for (slot, value) in segments.iter_mut().zip(left.iter().copied()) {
                *slot = value;
            }
        }
        Ok(Self::from_segments(segments))
    }
}

fn parse_v6_groups(text: &str) -> Result<alloc::vec::Vec<u16>, AddrError> {
    if text.is_empty() {
        return Ok(alloc::vec::Vec::new());
    }
    let mut groups = alloc::vec::Vec::new();
    for part in text.split(':') {
        if part.is_empty() || part.len() > 4 {
            return Err(AddrError::BadAddress);
        }
        let mut value = 0u16;
        for byte in part.bytes() {
            let digit = match byte {
                b'0'..=b'9' => u16::from(byte - b'0'),
                b'a'..=b'f' => u16::from(byte - b'a') + 10,
                b'A'..=b'F' => u16::from(byte - b'A') + 10,
                _ => return Err(AddrError::BadAddress),
            };
            value = (value << 4) | digit;
        }
        groups.push(value);
    }
    Ok(groups)
}

impl fmt::Display for Ipv6Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let groups = self.segments();
        let mut best_start = 0usize;
        let mut best_len = 0usize;
        let mut index = 0usize;
        while index < 8 {
            if groups[index] != 0 {
                index += 1;
                continue;
            }
            let start = index;
            while index < 8 && groups[index] == 0 {
                index += 1;
            }
            let len = index - start;
            if len > best_len {
                best_start = start;
                best_len = len;
            }
        }
        if best_len < 2 {
            return write_groups(f, &groups, 0, 8);
        }
        let mut text = String::new();
        write_groups_into(&mut text, &groups, 0, best_start);
        text.push_str("::");
        write_groups_into(&mut text, &groups, best_start + best_len, 8);
        f.write_str(&text)
    }
}

fn write_groups(
    f: &mut fmt::Formatter<'_>,
    groups: &[u16; 8],
    start: usize,
    end: usize,
) -> fmt::Result {
    let mut text = String::new();
    write_groups_into(&mut text, groups, start, end);
    f.write_str(&text)
}

fn write_groups_into(text: &mut String, groups: &[u16; 8], start: usize, end: usize) {
    for (position, group) in groups.iter().enumerate().take(end).skip(start) {
        if position > start {
            text.push(':');
        }
        text.push_str(&format!("{group:x}"));
    }
}

/// Either address family.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IpAddr {
    /// An IPv4 address.
    V4(Ipv4Addr),
    /// An IPv6 address.
    V6(Ipv6Addr),
}

impl IpAddr {
    /// Parse an IPv4 or IPv6 literal.
    ///
    /// # Errors
    ///
    /// Returns whatever the family parser returns.
    pub fn parse(text: &str) -> Result<Self, AddrError> {
        if text.contains(':') {
            Ipv6Addr::parse(text).map(Self::V6)
        } else {
            Ipv4Addr::parse(text).map(Self::V4)
        }
    }

    /// True for the IPv4 family.
    #[must_use]
    pub const fn is_v4(self) -> bool {
        matches!(self, Self::V4(_))
    }

    /// True for the IPv6 family.
    #[must_use]
    pub const fn is_v6(self) -> bool {
        matches!(self, Self::V6(_))
    }

    /// The address family, as a prefix length bound.
    #[must_use]
    pub const fn prefix_bits(self) -> u8 {
        match self {
            Self::V4(_) => 32,
            Self::V6(_) => 128,
        }
    }

    /// Clear every bit after `prefix` bits.
    #[must_use]
    pub fn masked(self, prefix: u8) -> Self {
        match self {
            Self::V4(address) => {
                let prefix = prefix.min(32);
                if prefix == 0 {
                    return Self::V4(Ipv4Addr::from_u32(0));
                }
                let mask = u32::MAX << (32 - u32::from(prefix));
                Self::V4(Ipv4Addr::from_u32(address.to_u32() & mask))
            }
            Self::V6(address) => {
                let prefix = prefix.min(128);
                let mut octets = address.octets();
                let full_bytes = usize::from(prefix / 8);
                let extra = prefix % 8;
                for (index, octet) in octets.iter_mut().enumerate() {
                    if index < full_bytes {
                        continue;
                    }
                    if index == full_bytes && extra != 0 {
                        *octet &= 0xff << (8 - extra);
                    } else {
                        *octet = 0;
                    }
                }
                Self::V6(Ipv6Addr::new(octets))
            }
        }
    }

    /// True when `self` shares the first `prefix` bits with `network`.
    #[must_use]
    pub fn in_network(self, network: Self, prefix: u8) -> bool {
        if self.is_v4() != network.is_v4() {
            return false;
        }
        self.masked(prefix) == network && network == network.masked(prefix)
    }

    /// The host bits as an integer, used by deterministic pool selection.
    #[must_use]
    pub fn low_u64(self) -> u64 {
        match self {
            Self::V4(address) => u64::from(address.to_u32()),
            Self::V6(address) => {
                let octets = address.octets();
                let mut value = 0u64;
                for byte in octets {
                    value = (value << 8) | u64::from(byte);
                }
                value
            }
        }
    }
}

impl From<Ipv4Addr> for IpAddr {
    fn from(address: Ipv4Addr) -> Self {
        Self::V4(address)
    }
}

impl From<Ipv6Addr> for IpAddr {
    fn from(address: Ipv6Addr) -> Self {
        Self::V6(address)
    }
}

impl fmt::Display for IpAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::V4(address) => address.fmt(f),
            Self::V6(address) => address.fmt(f),
        }
    }
}

/// One address condition in a rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddrSpec {
    /// Any address.
    Any,
    /// Exactly one host.
    Host(IpAddr),
    /// A CIDR network; `network` is already masked.
    Network {
        /// Network address with the host bits cleared.
        network: IpAddr,
        /// Prefix length, `0..=32` for IPv4 and `0..=128` for IPv6.
        prefix: u8,
    },
    /// A reference to [`crate::table::Table`] by index.
    Table(u16),
}

impl AddrSpec {
    /// Parse `any`, an IPv4/IPv6 literal or `address/prefix`.
    ///
    /// # Errors
    ///
    /// Returns [`AddrError`] for a malformed address or an out-of-range prefix.
    pub fn parse(text: &str) -> Result<Self, AddrError> {
        if text.eq_ignore_ascii_case("any") {
            return Ok(Self::Any);
        }
        match text.split_once('/') {
            Some((address, prefix)) => {
                let address = IpAddr::parse(address)?;
                let prefix = parse_prefix(prefix, address.prefix_bits())?;
                Ok(Self::network(address, prefix))
            }
            None => Ok(Self::Host(IpAddr::parse(text)?)),
        }
    }

    /// Build a normalized network specification.
    #[must_use]
    pub fn network(address: IpAddr, prefix: u8) -> Self {
        let prefix = prefix.min(address.prefix_bits());
        Self::Network {
            network: address.masked(prefix),
            prefix,
        }
    }

    /// True when `address` satisfies this condition.
    #[must_use]
    pub fn matches(self, address: impl Into<IpAddr>, tables: &[Table]) -> bool {
        self.matches_depth(address.into(), tables, 0)
    }

    pub(crate) fn matches_depth(self, address: IpAddr, tables: &[Table], depth: u8) -> bool {
        match self {
            Self::Any => true,
            Self::Host(host) => host == address,
            Self::Network { network, prefix } => address.in_network(network, prefix),
            Self::Table(index) => {
                if depth >= MATCH_DEPTH_LIMIT {
                    return false;
                }
                tables
                    .get(usize::from(index))
                    .is_some_and(|table| table.matches_at(address, tables, depth + 1))
            }
        }
    }
}

fn parse_prefix(text: &str, max: u8) -> Result<u8, AddrError> {
    if text.is_empty() || text.len() > 3 {
        return Err(AddrError::BadPrefix);
    }
    let mut value = 0u16;
    for byte in text.bytes() {
        if !byte.is_ascii_digit() {
            return Err(AddrError::BadPrefix);
        }
        value = value * 10 + u16::from(byte - b'0');
    }
    if value > u16::from(max) {
        return Err(AddrError::BadPrefix);
    }
    u8::try_from(value).map_err(|_| AddrError::BadPrefix)
}

/// One port condition in a rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortSpec {
    /// Any port.
    Any,
    /// Exactly this port.
    Equal(u16),
    /// Any port except this one.
    NotEqual(u16),
    /// Ports below the value.
    Less(u16),
    /// Ports above the value.
    Greater(u16),
    /// Inclusive range.
    Range {
        /// First port.
        start: u16,
        /// Last port.
        end: u16,
    },
}

impl PortSpec {
    /// Parse `any`, `N`, `A:B`, `< N`, `> N` or `!= N`.
    ///
    /// # Errors
    ///
    /// Returns [`AddrError::BadPort`] for a malformed or out-of-range port.
    pub fn parse(text: &str) -> Result<Self, AddrError> {
        if text.eq_ignore_ascii_case("any") {
            return Ok(Self::Any);
        }
        if let Some(rest) = text.strip_prefix("!=") {
            return parse_port(rest).map(Self::NotEqual);
        }
        if let Some(rest) = text.strip_prefix('<') {
            return parse_port(rest).map(Self::Less);
        }
        if let Some(rest) = text.strip_prefix('>') {
            return parse_port(rest).map(Self::Greater);
        }
        if let Some((start, end)) = text.split_once(':') {
            let start = if start.is_empty() {
                0
            } else {
                parse_port(start)?
            };
            let end = if end.is_empty() {
                u16::MAX
            } else {
                parse_port(end)?
            };
            if start > end {
                return Err(AddrError::BadPort);
            }
            return Ok(Self::Range { start, end });
        }
        parse_port(text).map(Self::Equal)
    }

    /// True when `port` satisfies this condition.
    #[must_use]
    pub fn matches(self, port: u16) -> bool {
        match self {
            Self::Any => true,
            Self::Equal(want) => port == want,
            Self::NotEqual(want) => port != want,
            Self::Less(limit) => port < limit,
            Self::Greater(limit) => port > limit,
            Self::Range { start, end } => port >= start && port <= end,
        }
    }

    /// The inclusive `(low, high)` bounds, when the condition names a single span.
    #[must_use]
    pub fn span(self) -> Option<(u16, u16)> {
        match self {
            Self::Equal(port) => Some((port, port)),
            Self::Range { start, end } => Some((start, end)),
            _ => None,
        }
    }
}

fn parse_port(text: &str) -> Result<u16, AddrError> {
    if text.is_empty() || text.len() > 5 {
        return Err(AddrError::BadPort);
    }
    let mut value = 0u32;
    for byte in text.bytes() {
        if !byte.is_ascii_digit() {
            return Err(AddrError::BadPort);
        }
        value = value * 10 + u32::from(byte - b'0');
    }
    u16::try_from(value).map_err(|_| AddrError::BadPort)
}

/// Why an address or port specification was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddrError {
    /// Not a valid IPv4 or IPv6 address.
    BadAddress,
    /// The CIDR prefix is out of range for the family.
    BadPrefix,
    /// The port is not `0..=65535`.
    BadPort,
}

impl fmt::Display for AddrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadAddress => "invalid IP address",
            Self::BadPrefix => "invalid CIDR prefix",
            Self::BadPort => "invalid port",
        })
    }
}

#[cfg(test)]
#[path = "address_ut.rs"]
mod tests;

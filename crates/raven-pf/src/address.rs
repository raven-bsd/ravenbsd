// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! IPv4 address and port specifications for pf-like rules.

use core::fmt;

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

/// One address condition in a rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddrSpec {
    /// Any address.
    Any,
    /// Exactly one host.
    Host(Ipv4Addr),
    /// A CIDR network; `addr` is already masked.
    Network {
        /// Network address with the host bits cleared.
        network: Ipv4Addr,
        /// Prefix length, `0..=32`.
        prefix: u8,
    },
}

impl AddrSpec {
    /// Parse `any`, a dotted quad or `a.b.c.d/prefix`.
    ///
    /// # Errors
    ///
    /// Returns [`AddrError`] for a malformed address or a prefix above 32.
    pub fn parse(text: &str) -> Result<Self, AddrError> {
        if text.eq_ignore_ascii_case("any") {
            return Ok(Self::Any);
        }
        match text.split_once('/') {
            Some((address, prefix)) => {
                let address = Ipv4Addr::parse(address)?;
                let prefix = parse_prefix(prefix).ok_or(AddrError::BadPrefix)?;
                Ok(Self::network(address, prefix))
            }
            None => Ok(Self::Host(Ipv4Addr::parse(text)?)),
        }
    }

    /// Build a normalized network specification.
    #[must_use]
    pub fn network(address: Ipv4Addr, prefix: u8) -> Self {
        let prefix = prefix.min(32);
        if prefix == 0 {
            return Self::Network {
                network: Ipv4Addr::new(0, 0, 0, 0),
                prefix,
            };
        }
        let mask = u32::MAX << (32 - u32::from(prefix));
        Self::Network {
            network: Ipv4Addr::from_u32(address.to_u32() & mask),
            prefix,
        }
    }

    /// True when `address` satisfies this condition.
    #[must_use]
    pub fn matches(self, address: Ipv4Addr) -> bool {
        match self {
            Self::Any => true,
            Self::Host(host) => host == address,
            Self::Network { network, prefix } => {
                if prefix == 0 {
                    return true;
                }
                let mask = u32::MAX << (32 - u32::from(prefix));
                address.to_u32() & mask == network.to_u32()
            }
        }
    }
}

fn parse_prefix(text: &str) -> Option<u8> {
    if text.is_empty() || text.len() > 2 {
        return None;
    }
    let mut value = 0u16;
    for byte in text.bytes() {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value * 10 + u16::from(byte - b'0');
    }
    if value > 32 {
        return None;
    }
    u8::try_from(value).ok()
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
    /// Not a dotted-quad IPv4 address.
    BadAddress,
    /// The CIDR prefix is not `0..=32`.
    BadPrefix,
    /// The port is not `0..=65535`.
    BadPort,
}

impl fmt::Display for AddrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadAddress => "invalid IPv4 address",
            Self::BadPrefix => "invalid CIDR prefix",
            Self::BadPort => "invalid port",
        })
    }
}

#[cfg(test)]
#[path = "address_ut.rs"]
mod tests;

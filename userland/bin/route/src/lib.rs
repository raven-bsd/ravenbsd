// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Argument parsing for the RavenBSD `route` utility.
//!
//! The target program uses the B12d `RouteList`, `RouteAddOn` and `RouteDelOn` control requests;
//! this crate keeps the command grammar, address validation and destination canonicalisation
//! testable on the host.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use raven_proto_net::{ROUTE_GATEWAY, ROUTE_HOST, ROUTE_UP};

/// The interface used when the command omits one.
pub const DEFAULT_IFACE: &str = "lo";

/// One route to add.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteSpec<'a> {
    /// Destination address, already masked to the prefix length.
    pub dest: [u8; 4],
    /// Destination prefix length in bits.
    pub prefix: u8,
    /// Optional next hop.
    pub gateway: Option<[u8; 4]>,
    /// Interface carrying the route.
    pub iface: &'a str,
}

/// One `route` operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command<'a> {
    /// List the live route table.
    List,
    /// Add or replace one route.
    Add(RouteSpec<'a>),
    /// Delete one destination prefix.
    Delete {
        /// Destination address, already masked to the prefix length.
        dest: [u8; 4],
        /// Destination prefix length in bits.
        prefix: u8,
        /// Interface carrying the route.
        iface: &'a str,
    },
}

/// Why the arguments were rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// Wrong operands or an unknown command.
    Usage,
    /// The IPv4 address is malformed.
    BadAddress,
    /// The prefix is not `0..=32`.
    BadPrefix,
    /// The interface name is empty, too long or has a forbidden byte.
    BadInterface,
}

impl ParseError {
    /// Stable diagnostic text.
    pub const fn message(self) -> &'static str {
        match self {
            Self::Usage => {
                "usage: route [show] | route add default <gateway> [iface] | \
                 route add host <address> [iface] | route add <dest>[/prefix] [iface] | \
                 route delete default|host <address>|<dest>[/prefix] [iface]"
            }
            Self::BadAddress => "invalid IPv4 address",
            Self::BadPrefix => "invalid prefix length",
            Self::BadInterface => "invalid interface name",
        }
    }
}

/// Parse the arguments after `argv[0]`.
pub fn parse<'a>(args: &[&'a str]) -> Result<Command<'a>, ParseError> {
    match args {
        [] | ["show"] | ["list"] => Ok(Command::List),
        ["add", rest @ ..] => parse_add(rest),
        ["delete", rest @ ..] | ["del", rest @ ..] => parse_delete(rest),
        _ => Err(ParseError::Usage),
    }
}

fn parse_add<'a>(args: &[&'a str]) -> Result<Command<'a>, ParseError> {
    match args {
        ["default", gateway] => add_default(gateway, DEFAULT_IFACE),
        ["default", gateway, iface] => add_default(gateway, iface),
        ["default", ..] => Err(ParseError::Usage),
        ["host", address] => add_host(address, DEFAULT_IFACE),
        ["host", address, iface] => add_host(address, iface),
        ["host", ..] => Err(ParseError::Usage),
        [dest] => add_dest(dest, DEFAULT_IFACE),
        [dest, iface] => add_dest(dest, iface),
        _ => Err(ParseError::Usage),
    }
}

fn add_default<'a>(gateway: &str, iface: &'a str) -> Result<Command<'a>, ParseError> {
    validate_iface(iface)?;
    Ok(Command::Add(RouteSpec {
        dest: [0; 4],
        prefix: 0,
        gateway: Some(parse_ipv4(gateway)?),
        iface,
    }))
}

fn add_host<'a>(address: &str, iface: &'a str) -> Result<Command<'a>, ParseError> {
    validate_iface(iface)?;
    Ok(Command::Add(RouteSpec {
        dest: parse_ipv4(address)?,
        prefix: 32,
        gateway: None,
        iface,
    }))
}

fn add_dest<'a>(text: &str, iface: &'a str) -> Result<Command<'a>, ParseError> {
    validate_iface(iface)?;
    let (dest, prefix) = parse_destination(text)?;
    Ok(Command::Add(RouteSpec {
        dest,
        prefix,
        gateway: None,
        iface,
    }))
}

fn parse_delete<'a>(args: &[&'a str]) -> Result<Command<'a>, ParseError> {
    match args {
        ["default"] => delete_default(DEFAULT_IFACE),
        ["default", iface] => delete_default(iface),
        ["host", address] => delete_host(address, DEFAULT_IFACE),
        ["host", address, iface] => delete_host(address, iface),
        ["host", ..] => Err(ParseError::Usage),
        [dest] => delete_dest(dest, DEFAULT_IFACE),
        [dest, iface] => delete_dest(dest, iface),
        _ => Err(ParseError::Usage),
    }
}

fn delete_default<'a>(iface: &'a str) -> Result<Command<'a>, ParseError> {
    validate_iface(iface)?;
    Ok(Command::Delete {
        dest: [0; 4],
        prefix: 0,
        iface,
    })
}

fn delete_host<'a>(address: &str, iface: &'a str) -> Result<Command<'a>, ParseError> {
    validate_iface(iface)?;
    Ok(Command::Delete {
        dest: parse_ipv4(address)?,
        prefix: 32,
        iface,
    })
}

fn delete_dest<'a>(text: &str, iface: &'a str) -> Result<Command<'a>, ParseError> {
    validate_iface(iface)?;
    let (dest, prefix) = parse_destination(text)?;
    Ok(Command::Delete {
        dest,
        prefix,
        iface,
    })
}

/// Parse `address`, `address/prefix` or `default`; a bare address is a host route.
pub fn parse_destination(text: &str) -> Result<([u8; 4], u8), ParseError> {
    if text == "default" {
        return Ok(([0; 4], 0));
    }
    let (address, prefix) = match text.split_once('/') {
        Some((address, prefix)) => (address, parse_prefix(prefix)?),
        None => (text, 32),
    };
    let address = parse_ipv4(address)?;
    Ok((canonical(address, prefix), prefix))
}

/// Mask `address` to `prefix`.
pub fn canonical(address: [u8; 4], prefix: u8) -> [u8; 4] {
    let mask = if prefix == 0 {
        0
    } else if prefix >= 32 {
        u32::MAX
    } else {
        u32::MAX << (32 - u32::from(prefix))
    };
    (u32::from_be_bytes(address) & mask).to_be_bytes()
}

/// Parse a strict dotted-quad IPv4 address.
pub fn parse_ipv4(text: &str) -> Result<[u8; 4], ParseError> {
    let mut octets = [0u8; 4];
    let mut parts = text.split('.');
    for octet in &mut octets {
        let part = parts.next().ok_or(ParseError::BadAddress)?;
        if part.is_empty() || part.len() > 3 || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(ParseError::BadAddress);
        }
        if part.len() > 1 && part.starts_with('0') {
            return Err(ParseError::BadAddress);
        }
        let value: u16 = part.parse().map_err(|_| ParseError::BadAddress)?;
        if value > u16::from(u8::MAX) {
            return Err(ParseError::BadAddress);
        }
        *octet = value as u8;
    }
    if parts.next().is_some() {
        return Err(ParseError::BadAddress);
    }
    Ok(octets)
}

fn parse_prefix(text: &str) -> Result<u8, ParseError> {
    if text.is_empty() || text.len() > 2 || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ParseError::BadPrefix);
    }
    if text.len() > 1 && text.starts_with('0') {
        return Err(ParseError::BadPrefix);
    }
    let value: u8 = text.parse().map_err(|_| ParseError::BadPrefix)?;
    if value > 32 {
        return Err(ParseError::BadPrefix);
    }
    Ok(value)
}

/// Validate an interface name (`1..=16` ASCII letters/digits/underscore/hyphen/dot).
pub fn validate_iface(name: &str) -> Result<(), ParseError> {
    if name.is_empty()
        || name.len() > 16
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(ParseError::BadInterface);
    }
    Ok(())
}

/// Human-readable route-list flag list for the wire flag byte.
pub fn flags_text(flags: u8) -> &'static str {
    match flags & (ROUTE_UP | ROUTE_GATEWAY | ROUTE_HOST) {
        0 => "-",
        1 => "UP",
        2 => "GATEWAY",
        3 => "UP,GATEWAY",
        4 => "HOST",
        5 => "UP,HOST",
        6 => "GATEWAY,HOST",
        _ => "UP,GATEWAY,HOST",
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

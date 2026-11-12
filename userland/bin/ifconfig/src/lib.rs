// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Argument parsing and display helpers for `ifconfig`.
//!
//! The target program talks only to the `net:` control channel; this crate keeps the parsing
//! and formatting testable on the host.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// One `ifconfig` operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command<'a> {
    /// List all interfaces.
    List,
    /// Show one interface.
    Get(&'a str),
    /// Set an address and prefix length.
    SetAddress(&'a str, [u8; 4], u8),
    /// Bring an interface up or down.
    SetState(&'a str, bool),
}

/// Why the arguments were rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// Wrong number of arguments.
    Usage,
    /// The interface name is empty, too long or has a forbidden byte.
    BadName,
    /// The IPv4 address is not four decimal octets in `0..=255`.
    BadAddress,
    /// The prefix length is not `0..=32`.
    BadPrefix,
}

impl ParseError {
    /// Stable diagnostic text.
    pub const fn message(self) -> &'static str {
        match self {
            Self::Usage => "usage: ifconfig [interface [address[/prefix] | up | down]]",
            Self::BadName => "invalid interface name",
            Self::BadAddress => "invalid IPv4 address",
            Self::BadPrefix => "invalid prefix length",
        }
    }
}

/// Validate an interface name (`1..=16` ASCII letters/digits/underscore/hyphen/dot).
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 16
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

/// Parse four decimal IPv4 octets.
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

/// Parse `address` or `address/prefix`; the default prefix is 24.
pub fn parse_address(text: &str) -> Result<([u8; 4], u8), ParseError> {
    let (address, prefix) = match text.split_once('/') {
        Some((address, prefix)) => (address, Some(prefix)),
        None => (text, None),
    };
    let address = parse_ipv4(address)?;
    let prefix = match prefix {
        Some(prefix) => prefix.parse::<u8>().map_err(|_| ParseError::BadPrefix)?,
        None => 24,
    };
    if prefix > 32 {
        return Err(ParseError::BadPrefix);
    }
    Ok((address, prefix))
}

/// Parse the arguments after `argv[0]`.
pub fn parse<'a>(args: &[&'a str]) -> Result<Command<'a>, ParseError> {
    match args {
        [] | [""] => Ok(Command::List),
        [name] if valid_name(name) => Ok(Command::Get(name)),
        [name, state] if valid_name(name) && (*state == "up" || *state == "down") => {
            Ok(Command::SetState(name, *state == "up"))
        }
        [name, address] if valid_name(name) => {
            let (address, prefix) = parse_address(address)?;
            Ok(Command::SetAddress(name, address, prefix))
        }
        _ => Err(ParseError::Usage),
    }
}

/// Dotted netmask for a prefix length.
pub fn netmask(prefix: u8) -> [u8; 4] {
    if prefix == 0 {
        return [0; 4];
    }
    if prefix >= 32 {
        return [255; 4];
    }
    let mask = u32::MAX << (32 - u32::from(prefix));
    mask.to_be_bytes()
}

/// Human-readable flag list for the wire flag byte.
pub fn flags_text(flags: u8) -> &'static str {
    match flags & 0x7 {
        0 => "",
        1 => "UP",
        2 => "LINK_UP",
        3 => "UP,LINK_UP",
        4 => "LOOPBACK",
        5 => "UP,LOOPBACK",
        6 => "LINK_UP,LOOPBACK",
        _ => "UP,LINK_UP,LOOPBACK",
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

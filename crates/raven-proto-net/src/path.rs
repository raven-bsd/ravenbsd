// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Scheme paths for `net:`: socket transports and the interface control channel.
//!
//! The native network namespace mirrors the shape of the other schemes:
//!
//! * `net:tcp/<ipv4>:<port>` - a TCP socket to a remote endpoint;
//! * `net:udp/<ipv4>:<port>` - a UDP socket;
//! * `net:` - the control channel (interface table, routes, ARP).
//!
//! Parsing is strict: a malformed path is a [`PathError`], never a best-effort guess.

use core::fmt;

use crate::socket::Endpoint;

/// Transport selected by an [`SchemePath`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    /// A stream socket.
    Tcp,
    /// A datagram socket.
    Udp,
    /// The interface control channel.
    Control,
}

/// A parsed `net:` scheme path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SchemePath {
    /// Selected transport.
    pub transport: Transport,
    /// Remote endpoint; present for `tcp` and `udp`, absent for the control channel.
    pub endpoint: Option<Endpoint>,
}

/// Why a `net:` path was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathError {
    /// The path does not start with `net:`.
    NotNet,
    /// The part after `net:` is neither `tcp/`, `udp/` nor empty.
    UnknownTransport,
    /// The endpoint has no `:` between address and port, or lacks an IPv4 address.
    MissingAddress,
    /// The IPv4 address is not four decimal octets in `0..=255`.
    BadAddress,
    /// The port is not a decimal number in `1..=65535`.
    BadPort,
}

impl fmt::Display for PathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotNet => "not a net: scheme path",
            Self::UnknownTransport => "unknown net: transport",
            Self::MissingAddress => "missing address or port",
            Self::BadAddress => "invalid IPv4 address",
            Self::BadPort => "invalid port",
        })
    }
}

/// Prefix used by an accepted TCP socket's client-side file open.
pub const ACCEPT_PREFIX: &str = "net:accept/";

/// Parse the accepted-socket path `net:accept/<decimal socket id>`.
///
/// This is the ABI-v1 materialisation path for an `ACCEPT` result: the kernel has no
/// server-side `SchemeFile`-create primitive, so the client opens this path to turn the numeric
/// socket id into a scheme file. It is deliberately separate from [`parse`].
pub fn parse_accepted(path: &str) -> Option<u32> {
    let digits = path.strip_prefix(ACCEPT_PREFIX)?;
    if digits.is_empty()
        || digits.len() > 10
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
        || (digits.len() > 1 && digits.starts_with('0'))
    {
        return None;
    }
    digits.parse().ok()
}

/// Parse a `net:` scheme path.
pub fn parse(path: &str) -> Result<SchemePath, PathError> {
    let rest = path.strip_prefix("net:").ok_or(PathError::NotNet)?;
    if rest.is_empty() {
        return Ok(SchemePath {
            transport: Transport::Control,
            endpoint: None,
        });
    }
    let (transport, rest) = if let Some(rest) = rest.strip_prefix("tcp/") {
        (Transport::Tcp, rest)
    } else if let Some(rest) = rest.strip_prefix("udp/") {
        (Transport::Udp, rest)
    } else {
        return Err(PathError::UnknownTransport);
    };
    Ok(SchemePath {
        transport,
        endpoint: Some(parse_endpoint(rest)?),
    })
}

/// Parse `<ipv4>:<port>`.
fn parse_endpoint(text: &str) -> Result<Endpoint, PathError> {
    let (address, port) = text.split_once(':').ok_or(PathError::MissingAddress)?;
    Ok(Endpoint {
        ip: parse_ipv4(address)?,
        port: parse_port(port)?,
    })
}

/// Parse four decimal octets separated by dots.
fn parse_ipv4(text: &str) -> Result<[u8; 4], PathError> {
    let mut octets = [0u8; 4];
    let mut parts = text.split('.');
    for octet in &mut octets {
        let part = parts.next().ok_or(PathError::BadAddress)?;
        if part.is_empty() || part.len() > 3 || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(PathError::BadAddress);
        }
        if part.len() > 1 && part.starts_with('0') {
            return Err(PathError::BadAddress);
        }
        let value: u16 = part.parse().map_err(|_| PathError::BadAddress)?;
        if value > u8::MAX as u16 {
            return Err(PathError::BadAddress);
        }
        *octet = value as u8;
    }
    if parts.next().is_some() {
        return Err(PathError::BadAddress);
    }
    Ok(octets)
}

/// Parse a decimal TCP/UDP port in `1..=65535`.
fn parse_port(text: &str) -> Result<u16, PathError> {
    if text.is_empty() || text.len() > 5 || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(PathError::BadPort);
    }
    if text.len() > 1 && text.starts_with('0') {
        return Err(PathError::BadPort);
    }
    let port: u32 = text.parse().map_err(|_| PathError::BadPort)?;
    if port == 0 || port > u16::MAX as u32 {
        return Err(PathError::BadPort);
    }
    Ok(port as u16)
}

#[cfg(test)]
#[path = "path_ut.rs"]
mod tests;

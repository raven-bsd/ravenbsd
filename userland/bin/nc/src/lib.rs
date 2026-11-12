// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Argument parsing for the RavenBSD `nc` utility.
//!
//! The wire client is target-only; this crate keeps the option grammar and bounds testable.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Largest payload accepted from `-s`.
pub const MAX_SEND: usize = 1024;
/// Largest `-n` read request.
pub const MAX_READ: usize = 1024;

/// Parsed `nc` options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options<'a> {
    /// Use UDP instead of TCP.
    pub udp: bool,
    /// Listen instead of connecting.
    pub listen: bool,
    /// Echo one received message back (listener).
    pub echo: bool,
    /// Listener port (`-p`).
    pub port: Option<u16>,
    /// Client bind port (`-b`).
    pub bind: Option<u16>,
    /// Overall timeout in seconds.
    pub timeout_secs: u64,
    /// Payload to send once after connecting.
    pub send: Option<&'a str>,
    /// Maximum bytes to read.
    pub read_len: usize,
    /// Remote host (client).
    pub host: Option<&'a str>,
    /// Remote port (client).
    pub remote_port: Option<u16>,
}

impl<'a> Options<'a> {
    /// Default options: TCP client, 5 s timeout, 256-byte read.
    pub const fn new() -> Self {
        Self {
            udp: false,
            listen: false,
            echo: false,
            port: None,
            bind: None,
            timeout_secs: 5,
            send: None,
            read_len: 0,
            host: None,
            remote_port: None,
        }
    }

    /// Check that the selected mode has the required operands.
    pub fn validate(&self) -> Result<(), ParseError> {
        if self.timeout_secs == 0 || self.timeout_secs > 3600 {
            return Err(ParseError::BadTimeout);
        }
        if self.read_len > MAX_READ {
            return Err(ParseError::BadLength);
        }
        if let Some(send) = self.send
            && send.len() > MAX_SEND
        {
            return Err(ParseError::BadPayload);
        }
        if self.listen {
            if self.port.is_none() {
                return Err(ParseError::MissingPort);
            }
            if self.host.is_some() || self.remote_port.is_some() {
                return Err(ParseError::Usage);
            }
        } else {
            if self.host.is_none() || self.remote_port.is_none() {
                return Err(ParseError::Usage);
            }
            if self.udp && self.bind.is_none() {
                return Err(ParseError::MissingBind);
            }
        }
        Ok(())
    }
}

impl Default for Options<'_> {
    fn default() -> Self {
        Self::new()
    }
}

/// Why the arguments were rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// Wrong operands for the selected mode.
    Usage,
    /// An option needs a value.
    MissingValue,
    /// The port is not `1..=65535`.
    BadPort,
    /// The timeout is not `1..=3600`.
    BadTimeout,
    /// The read length is not `1..=MAX_READ`.
    BadLength,
    /// The send payload is too large.
    BadPayload,
    /// A listener needs `-p`.
    MissingPort,
    /// A UDP client needs `-b`.
    MissingBind,
    /// The IPv4 address is malformed.
    BadAddress,
}

impl ParseError {
    /// Stable diagnostic text.
    pub const fn message(self) -> &'static str {
        match self {
            Self::Usage => {
                "usage: nc [-u] [-l [-p port]] [-b port] [-w secs] [-s text] [-n bytes] [-e] [host port]"
            }
            Self::MissingValue => "option requires a value",
            Self::BadPort => "invalid port",
            Self::BadTimeout => "invalid timeout",
            Self::BadLength => "invalid read length",
            Self::BadPayload => "send payload is too large",
            Self::MissingPort => "a listener needs -p <port>",
            Self::MissingBind => "a UDP client needs -b <local-port>",
            Self::BadAddress => "invalid IPv4 address",
        }
    }
}

/// Parse the arguments after `argv[0]`.
pub fn parse<'a>(args: &[&'a str]) -> Result<Options<'a>, ParseError> {
    let mut options = Options::new();
    let mut positionals: [Option<&'a str>; 2] = [None, None];
    let mut positional_count = 0usize;
    let mut index = 0usize;
    while index < args.len() {
        let argument = args.get(index).copied().ok_or(ParseError::Usage)?;
        match argument {
            "-u" => options.udp = true,
            "-l" => options.listen = true,
            "-e" => options.echo = true,
            "-w" => {
                index += 1;
                let value = args.get(index).copied().ok_or(ParseError::MissingValue)?;
                options.timeout_secs = value.parse().map_err(|_| ParseError::BadTimeout)?;
            }
            "-p" => {
                index += 1;
                let value = args.get(index).copied().ok_or(ParseError::MissingValue)?;
                options.port = Some(parse_port(value)?);
            }
            "-b" => {
                index += 1;
                let value = args.get(index).copied().ok_or(ParseError::MissingValue)?;
                options.bind = Some(parse_port(value)?);
            }
            "-s" => {
                index += 1;
                options.send = Some(args.get(index).copied().ok_or(ParseError::MissingValue)?);
            }
            "-n" => {
                index += 1;
                let value = args.get(index).copied().ok_or(ParseError::MissingValue)?;
                options.read_len = value.parse().map_err(|_| ParseError::BadLength)?;
            }
            "-h" | "--help" => return Err(ParseError::Usage),
            value if value.starts_with('-') => return Err(ParseError::Usage),
            value => {
                let slot = positionals
                    .get_mut(positional_count)
                    .ok_or(ParseError::Usage)?;
                *slot = Some(value);
                positional_count += 1;
            }
        }
        index += 1;
    }
    if options.listen {
        if positional_count != 0 {
            return Err(ParseError::Usage);
        }
    } else {
        if positional_count != 2 {
            return Err(ParseError::Usage);
        }
        options.host = positionals[0];
        options.remote_port = Some(parse_port(positionals[1].ok_or(ParseError::Usage)?)?);
        if !valid_ipv4(options.host.unwrap_or("")) {
            return Err(ParseError::BadAddress);
        }
    }
    options.validate()?;
    Ok(options)
}

/// Parse a decimal port in `1..=65535`.
pub fn parse_port(text: &str) -> Result<u16, ParseError> {
    if text.is_empty() || text.len() > 5 || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ParseError::BadPort);
    }
    if text.len() > 1 && text.starts_with('0') {
        return Err(ParseError::BadPort);
    }
    let port: u32 = text.parse().map_err(|_| ParseError::BadPort)?;
    if port == 0 || port > u32::from(u16::MAX) {
        return Err(ParseError::BadPort);
    }
    Ok(port as u16)
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

/// A strict dotted-quad IPv4 check.
pub fn valid_ipv4(text: &str) -> bool {
    parse_ipv4(text).is_ok()
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Argument parsing and bounds for the RavenBSD `ping` utility.
//!
//! The target program drives the B12d `net:icmp/<ipv4>` socket; this crate keeps the option
//! grammar and the IPv4 parser testable on the host.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Default echo count.
pub const DEFAULT_COUNT: u16 = 4;
/// Default overall timeout in seconds.
pub const DEFAULT_TIMEOUT_SECS: u64 = 5;
/// Default payload length in bytes.
pub const DEFAULT_PAYLOAD: usize = 32;
/// Largest accepted echo count.
pub const MAX_COUNT: u16 = 1000;
/// Largest accepted timeout in seconds.
pub const MAX_TIMEOUT_SECS: u64 = 60;
/// Largest accepted payload, matching the protocol's ICMP bound.
pub const MAX_PAYLOAD: usize = 1400;

/// Parsed `ping` options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options<'a> {
    /// Destination IPv4 address.
    pub host: &'a str,
    /// Number of echo requests to send.
    pub count: u16,
    /// Overall budget for the whole run in seconds.
    pub timeout_secs: u64,
    /// Payload bytes in each echo request.
    pub payload_len: usize,
}

impl<'a> Options<'a> {
    /// Default options for `host`.
    pub const fn new(host: &'a str) -> Self {
        Self {
            host,
            count: DEFAULT_COUNT,
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            payload_len: DEFAULT_PAYLOAD,
        }
    }
}

/// Why the arguments were rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// Wrong operands or an unknown option.
    Usage,
    /// An option needs a value.
    MissingValue,
    /// The count is not `1..=MAX_COUNT`.
    BadCount,
    /// The timeout is not `1..=MAX_TIMEOUT_SECS`.
    BadTimeout,
    /// The payload is larger than `MAX_PAYLOAD`.
    BadPayload,
    /// The destination is not a dotted-quad IPv4 address.
    BadAddress,
}

impl ParseError {
    /// Stable diagnostic text.
    pub const fn message(self) -> &'static str {
        match self {
            Self::Usage => "usage: ping [-c count] [-w seconds] [-s bytes] host",
            Self::MissingValue => "option requires a value",
            Self::BadCount => "invalid echo count",
            Self::BadTimeout => "invalid timeout",
            Self::BadPayload => "invalid payload size",
            Self::BadAddress => "invalid IPv4 address",
        }
    }
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

/// Whether `text` is a strict dotted-quad IPv4 address.
pub fn valid_ipv4(text: &str) -> bool {
    parse_ipv4(text).is_ok()
}

/// Parse the arguments after `argv[0]`.
pub fn parse<'a>(args: &[&'a str]) -> Result<Options<'a>, ParseError> {
    let mut count = DEFAULT_COUNT;
    let mut timeout_secs = DEFAULT_TIMEOUT_SECS;
    let mut payload_len = DEFAULT_PAYLOAD;
    let mut host = None;
    let mut index = 0usize;
    while index < args.len() {
        let argument = args.get(index).copied().ok_or(ParseError::Usage)?;
        match argument {
            "-c" => {
                index += 1;
                let value = args.get(index).copied().ok_or(ParseError::MissingValue)?;
                let parsed = decimal(value).ok_or(ParseError::BadCount)?;
                if parsed == 0 || parsed > u64::from(MAX_COUNT) {
                    return Err(ParseError::BadCount);
                }
                count = parsed as u16;
            }
            "-w" => {
                index += 1;
                let value = args.get(index).copied().ok_or(ParseError::MissingValue)?;
                let parsed = decimal(value).ok_or(ParseError::BadTimeout)?;
                if parsed == 0 || parsed > MAX_TIMEOUT_SECS {
                    return Err(ParseError::BadTimeout);
                }
                timeout_secs = parsed;
            }
            "-s" => {
                index += 1;
                let value = args.get(index).copied().ok_or(ParseError::MissingValue)?;
                let parsed = decimal(value).ok_or(ParseError::BadPayload)?;
                if parsed > MAX_PAYLOAD as u64 {
                    return Err(ParseError::BadPayload);
                }
                payload_len = parsed as usize;
            }
            "-h" | "--help" => return Err(ParseError::Usage),
            value if value.starts_with('-') => return Err(ParseError::Usage),
            value => {
                if host.is_some() {
                    return Err(ParseError::Usage);
                }
                host = Some(value);
            }
        }
        index += 1;
    }
    let host = host.ok_or(ParseError::Usage)?;
    if !valid_ipv4(host) {
        return Err(ParseError::BadAddress);
    }
    Ok(Options {
        host,
        count,
        timeout_secs,
        payload_len,
    })
}

fn decimal(text: &str) -> Option<u64> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if text.len() > 1 && text.starts_with('0') {
        return None;
    }
    let mut value = 0u64;
    for byte in text.bytes() {
        value = value.checked_mul(10)?.checked_add(u64::from(byte - b'0'))?;
    }
    Some(value)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

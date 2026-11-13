// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Argument parsing and display names for the RavenBSD `netstat` utility.
//!
//! The target program reads the live B12d socket table and interface counters; this crate keeps
//! the option grammar and the wire-id-to-name mapping testable on the host.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Parsed `netstat` options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// Print the live socket table.
    pub sockets: bool,
    /// Print per-interface packet and byte counters.
    pub counters: bool,
}

impl Options {
    /// Default: print both the socket table and interface counters.
    pub const fn new() -> Self {
        Self {
            sockets: true,
            counters: true,
        }
    }
}

impl Default for Options {
    fn default() -> Self {
        Self::new()
    }
}

/// Why the arguments were rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// Wrong operands or an unknown option.
    Usage,
}

impl ParseError {
    /// Stable diagnostic text.
    pub const fn message(self) -> &'static str {
        match self {
            Self::Usage => "usage: netstat [-a] [-i | -s]",
        }
    }
}

/// Parse the arguments after `argv[0]`.
pub fn parse(args: &[&str]) -> Result<Options, ParseError> {
    let mut sockets = false;
    let mut counters = false;
    let mut explicit = false;
    for argument in args {
        match *argument {
            "-a" => {
                sockets = true;
                explicit = true;
            }
            "-i" | "-s" => {
                counters = true;
                explicit = true;
            }
            "-h" | "--help" => return Err(ParseError::Usage),
            _ => return Err(ParseError::Usage),
        }
    }
    if explicit {
        Ok(Options { sockets, counters })
    } else {
        Ok(Options::new())
    }
}

/// Display name for a socket transport id.
pub const fn transport_text(transport: u8) -> &'static str {
    match transport {
        1 => "tcp",
        2 => "udp",
        3 => "icmp",
        _ => "?",
    }
}

/// Display name for a socket state id.
pub const fn state_text(state: u8) -> &'static str {
    match state {
        0 => "CLOSED",
        1 => "LISTEN",
        2 => "SYN_SENT",
        3 => "SYN_RECEIVED",
        4 => "ESTABLISHED",
        5 => "FIN_WAIT_1",
        6 => "FIN_WAIT_2",
        7 => "CLOSE_WAIT",
        8 => "CLOSING",
        9 => "LAST_ACK",
        10 => "TIME_WAIT",
        11 => "BOUND",
        12 => "ECHO",
        _ => "?",
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

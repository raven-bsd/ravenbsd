// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure argument parsing and hostname validation for `hostname`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use core::fmt;

/// Longest accepted hostname (matches `sysd`).
pub const HOSTNAME_MAX: usize = 64;

/// What the command line asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action<'a> {
    /// Print the current hostname.
    Print,
    /// Set the hostname.
    Set(&'a str),
    /// Check the tool against the live `sys:` server.
    Selftest,
}

/// The command line was not a valid `hostname` invocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsageError;

impl fmt::Display for UsageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("usage: hostname [name] | hostname --selftest")
    }
}

/// Parse the arguments after `argv[0]`.
pub fn parse_args<'a>(args: impl Iterator<Item = &'a str>) -> Result<Action<'a>, UsageError> {
    let mut action = Action::Print;
    let mut seen = false;
    for argument in args {
        if seen {
            return Err(UsageError);
        }
        if argument == "--selftest" {
            action = Action::Selftest;
        } else if argument.starts_with('-') || !valid_name(argument) {
            return Err(UsageError);
        } else {
            action = Action::Set(argument);
        }
        seen = true;
    }
    Ok(action)
}

/// True when `name` is a non-empty, bounded, slash-free printable hostname.
pub fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= HOSTNAME_MAX
        && !bytes.contains(&0)
        && !bytes.contains(&b'/')
        && !bytes.iter().any(|byte| byte.is_ascii_control())
}

/// Remove one trailing `\n` or `\r\n` from a hostname file read.
pub fn trim_newline(bytes: &[u8]) -> &[u8] {
    let mut end = bytes.len();
    if end > 0 && bytes.get(end - 1) == Some(&b'\n') {
        end -= 1;
        if end > 0 && bytes.get(end - 1) == Some(&b'\r') {
            end -= 1;
        }
    }
    bytes.get(..end).unwrap_or(&[])
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

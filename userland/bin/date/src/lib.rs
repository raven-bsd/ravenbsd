// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure argument parsing and uptime formatting for `date`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use core::fmt;

/// What the command line asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Print seconds since boot.
    Print,
    /// Check the clock against the kernel.
    Selftest,
}

/// The command line was not a valid `date` invocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsageError;

impl fmt::Display for UsageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("usage: date | date --selftest")
    }
}

/// Parse the arguments after `argv[0]`.
pub fn parse_args<'a>(args: impl Iterator<Item = &'a str>) -> Result<Action, UsageError> {
    let mut action = Action::Print;
    let mut seen = false;
    for argument in args {
        if seen || (argument != "--selftest" && argument != "--") {
            return Err(UsageError);
        }
        if argument == "--selftest" {
            action = Action::Selftest;
        }
        seen = true;
    }
    Ok(action)
}

/// Write `up <seconds>s` (no wall clock exists in protocol version 1).
pub fn format_uptime(seconds: u64, out: &mut impl fmt::Write) -> fmt::Result {
    write!(out, "up {seconds}s")
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

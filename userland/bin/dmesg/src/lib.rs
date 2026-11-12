// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure argument parsing and record accounting for `dmesg`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use core::fmt;

/// What the command line asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Print the log ring.
    Print,
    /// Check the log server against the kernel.
    Selftest,
}

/// The command line was not a valid `dmesg` invocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsageError;

impl fmt::Display for UsageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("usage: dmesg | dmesg --selftest")
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

/// Number of newline-terminated records in `data`; a trailing fragment is not a record.
pub fn record_count(data: &[u8]) -> usize {
    data.iter().filter(|byte| **byte == b'\n').count()
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

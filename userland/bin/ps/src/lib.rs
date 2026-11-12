// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure argument parsing and `svc:` status formatting for `ps`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use core::fmt;

/// What the command line asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// List the services.
    List,
    /// Check the `svc:` server against the kernel.
    Selftest,
}

/// The command line was not a valid `ps` invocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsageError;

impl fmt::Display for UsageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("usage: ps | ps --selftest")
    }
}

/// The status line was not one of the documented states or fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatusError;

impl fmt::Display for StatusError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("bad service status line")
    }
}

const STATES: [&str; 6] = [
    "stopped",
    "starting",
    "running",
    "restarting",
    "failed",
    "gave-up",
];

/// Parse the arguments after `argv[0]`.
pub fn parse_args<'a>(args: impl Iterator<Item = &'a str>) -> Result<Action, UsageError> {
    let mut action = Action::List;
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

/// Format one `<name> <state> <handle> <restarts>` line as `NAME STATE RESTARTS`.
pub fn format_status(line: &str, out: &mut impl fmt::Write) -> Result<(), StatusError> {
    let mut fields = line.split_whitespace();
    let name = fields.next().ok_or(StatusError)?;
    let state = fields.next().ok_or(StatusError)?;
    let handle = fields.next().ok_or(StatusError)?;
    let restarts = fields.next().ok_or(StatusError)?;
    if fields.next().is_some() || !STATES.contains(&state) {
        return Err(StatusError);
    }
    if handle.parse::<u32>().is_err() || restarts.parse::<u32>().is_err() {
        return Err(StatusError);
    }
    writeln!(out, "{name} {state} {restarts}").map_err(|_| StatusError)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

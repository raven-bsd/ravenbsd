// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure argument parsing and `svc:` status formatting for `ps`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::string::String;
use core::fmt;

use raven_output::{JsonWriter, Record};

/// What the command line asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// List the services.
    List,
    /// List the services as NDJSON process records.
    ListJson,
    /// List POSIX process fields from the kernel registry.
    Posix,
    /// Check the `svc:` server against the kernel.
    Selftest,
}

/// The command line was not a valid `ps` invocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsageError;

impl fmt::Display for UsageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("usage: ps [--json] | ps --selftest")
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
///
/// `--json` and `--selftest` are mutually exclusive; `--` ends option parsing. A positional
/// argument, a repeated flag or an unknown flag is a usage error.
pub fn parse_args<'a>(args: impl Iterator<Item = &'a str>) -> Result<Action, UsageError> {
    let mut json = false;
    let mut selftest = false;
    let mut posix = false;
    let mut services = false;
    let mut ended = false;
    for argument in args {
        if ended {
            return Err(UsageError);
        }
        match argument {
            "--" => ended = true,
            "--json" if !json && !selftest && !posix && !services => json = true,
            "--selftest" if !selftest && !json && !posix && !services => selftest = true,
            "-e" | "--all" | "--posix" if !posix && !json && !selftest && !services => {
                posix = true;
            }
            "--services" if !services && !json && !selftest && !posix => services = true,
            _ => return Err(UsageError),
        }
    }
    Ok(if posix {
        Action::Posix
    } else if json {
        Action::ListJson
    } else if selftest {
        Action::Selftest
    } else {
        Action::List
    })
}

/// One parsed `svc:` status record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    /// Service name.
    pub name: String,
    /// One of the documented states.
    pub state: String,
    /// Service process handle, or zero when no process is running.
    pub handle: u32,
    /// Restart count.
    pub restarts: u32,
}

/// Parse `<name> <state> <handle> <restarts>`.
pub fn parse_status(line: &str) -> Result<Status, StatusError> {
    let mut fields = line.split_whitespace();
    let name = fields.next().ok_or(StatusError)?;
    let state = fields.next().ok_or(StatusError)?;
    let handle = fields.next().ok_or(StatusError)?;
    let restarts = fields.next().ok_or(StatusError)?;
    if fields.next().is_some() || !STATES.contains(&state) {
        return Err(StatusError);
    }
    let handle = handle.parse::<u32>().map_err(|_| StatusError)?;
    let restarts = restarts.parse::<u32>().map_err(|_| StatusError)?;
    Ok(Status {
        name: String::from(name),
        state: String::from(state),
        handle,
        restarts,
    })
}

/// Format one parsed status line as `NAME STATE RESTARTS`.
pub fn format_status(line: &str, out: &mut impl fmt::Write) -> Result<(), StatusError> {
    let status = parse_status(line)?;
    writeln!(out, "{} {} {}", status.name, status.state, status.restarts).map_err(|_| StatusError)
}

/// One POSIX process-table row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessRow {
    /// Process id.
    pub pid: i32,
    /// Parent process id.
    pub ppid: i32,
    /// Real user id.
    pub uid: u32,
    /// Real group id.
    pub gid: u32,
    /// Command name; `?` when the kernel registry has no path source.
    pub cmd: String,
}

/// Format the POSIX table header.
pub fn format_posix_header(out: &mut impl fmt::Write) -> fmt::Result {
    writeln!(out, "PID PPID UID GID CMD")
}

/// Format one POSIX table row as space-separated fields.
pub fn format_posix_row(row: &ProcessRow, out: &mut impl fmt::Write) -> fmt::Result {
    writeln!(
        out,
        "{} {} {} {} {}",
        row.pid, row.ppid, row.uid, row.gid, row.cmd
    )
}

/// The JSON record for one service-derived process entry.
///
/// The current `svc:` source exposes only the name, state and restart count. Process fields it
/// does not provide are `null` rather than fabricated values.
#[must_use]
pub fn process_record(status: &Status) -> Record {
    Record::new("process")
        .null("pid")
        .null("ppid")
        .null("uid")
        .null("gid")
        .string("state", &status.state)
        .string("name", &status.name)
        .null("command")
        .null("threads")
        .null("rss_bytes")
}

/// Format one status line as a single `kind:"process"` NDJSON object plus newline.
pub fn format_json_status(line: &str, out: &mut impl fmt::Write) -> Result<(), StatusError> {
    let status = parse_status(line)?;
    let mut writer = JsonWriter::new(out);
    writer
        .write_record(&process_record(&status))
        .map_err(|_| StatusError)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

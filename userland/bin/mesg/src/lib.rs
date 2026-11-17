// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure argument and mode handling for `mesg`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::string::{String, ToString};

/// The synopsis printed after `usage: mesg`.
pub const USAGE: &str = "[-t path] [y | n]";

/// The group-write mode bit that POSIX `mesg` controls.
pub const WRITE_BIT: u32 = 0o020;

/// A requested operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Print the current permission.
    Query,
    /// Allow (`true`) or refuse (`false`) terminal writes.
    Set(bool),
}

/// A parsed `mesg` command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// Terminal path; the default is `tty:console`.
    pub target: Option<String>,
    /// Requested operation.
    pub action: Action,
}

/// Why a `mesg` command line is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsageError;

/// Parse `mesg [-t path] [y | n]`.
pub fn parse_args<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Options, UsageError> {
    let mut target = None;
    let mut action = Action::Query;
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument {
            "-t" | "--tty" => {
                target = Some(args.next().ok_or(UsageError)?.to_string());
            }
            "y" | "yes" if action == Action::Query => action = Action::Set(true),
            "n" | "no" if action == Action::Query => action = Action::Set(false),
            _ => return Err(UsageError),
        }
    }
    Ok(Options { target, action })
}

/// True when the terminal mode allows group writes.
#[must_use]
pub const fn allows(mode: u32) -> bool {
    mode & WRITE_BIT != 0
}

/// Return `mode` with the group-write bit set or cleared.
#[must_use]
pub const fn apply(mode: u32, allowed: bool) -> u32 {
    if allowed {
        mode | WRITE_BIT
    } else {
        mode & !WRITE_BIT
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

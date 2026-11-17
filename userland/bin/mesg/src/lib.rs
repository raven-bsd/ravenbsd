// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure argument and mode handling for `mesg`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// The synopsis printed after `usage: mesg`.
pub const USAGE: &str = "[y | n]";

/// The group-write mode bit that POSIX `mesg` controls.
pub const WRITE_BIT: u32 = 0o020;

/// A parsed `mesg` command line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Print the current permission.
    Query,
    /// Allow (`true`) or refuse (`false`) terminal writes.
    Set(bool),
}

/// Why a `mesg` command line is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsageError;

/// Parse `mesg [y | n]`.
pub fn parse_args<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Action, UsageError> {
    let mut result = Action::Query;
    for argument in args {
        if result != Action::Query {
            return Err(UsageError);
        }
        result = match argument {
            "y" | "yes" => Action::Set(true),
            "n" | "no" => Action::Set(false),
            _ => return Err(UsageError),
        };
    }
    Ok(result)
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

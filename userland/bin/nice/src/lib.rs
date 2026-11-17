// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure argument handling for `nice`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// The synopsis printed after `usage: nice`.
pub const USAGE: &str = "[-n increment] [command [argument ...]]";

/// A parsed `nice` command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// Requested increment, or `None` for a query or the default +10 with a command.
    pub increment: Option<i32>,
    /// The command and its arguments, if any.
    pub command: Vec<String>,
}

/// Why a `nice` command line is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// Unknown option.
    UnknownOption,
    /// `-n` without a number or with a non-number.
    BadIncrement,
}

/// Parse `nice [-n increment] [command [argument ...]]`.
pub fn parse_args<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Options, UsageError> {
    let mut args = args.into_iter().peekable();
    let mut increment = None;
    let mut command = Vec::new();
    let mut parsing = true;
    while let Some(argument) = args.next() {
        if parsing && argument == "--" {
            parsing = false;
            continue;
        }
        if parsing && argument == "-n" {
            let Some(value) = args.next() else {
                return Err(UsageError::BadIncrement);
            };
            increment = Some(value.parse::<i32>().map_err(|_| UsageError::BadIncrement)?);
            continue;
        }
        if parsing && argument.starts_with('-') && argument != "-" {
            increment = Some(
                argument
                    .parse::<i32>()
                    .map_err(|_| UsageError::BadIncrement)?,
            );
            continue;
        }
        command.push(argument.to_string());
        command.extend(args.map(str::to_string));
        break;
    }
    Ok(Options { increment, command })
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

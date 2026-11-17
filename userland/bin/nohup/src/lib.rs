// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure argument handling for `nohup`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// The synopsis printed after `usage: nohup`.
pub const USAGE: &str = "command [argument ...]";

/// A parsed `nohup` command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// The command and its arguments.
    pub command: Vec<String>,
}

/// Why a `nohup` command line is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// No command was supplied.
    MissingCommand,
    /// An option was supplied; `nohup` takes no options.
    UnknownOption,
}

/// Parse `nohup [--] command [argument ...]`.
pub fn parse_args<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Options, UsageError> {
    let mut command = Vec::new();
    let mut parsing = true;
    for argument in args {
        if parsing && argument == "--" {
            parsing = false;
            continue;
        }
        if parsing && argument.starts_with('-') && argument != "-" {
            return Err(UsageError::UnknownOption);
        }
        parsing = false;
        command.push(argument.to_string());
    }
    if command.is_empty() {
        return Err(UsageError::MissingCommand);
    }
    Ok(Options { command })
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX `time` argument parsing and report formatting.
//!
//! The kernel exposes a monotonic clock but no per-process CPU accounting, so the target wrapper
//! reports the observed real time and `0.00` for user and sys. The format is the POSIX default and
//! `-p` form.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// Usage line.
pub const USAGE: &str = "time [-p] utility [argument ...]";

/// Argument errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// No utility operand.
    MissingUtility,
    /// Unknown option before the utility.
    UnknownOption,
}

/// Parsed command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options<'a> {
    /// Portable output format.
    pub portable: bool,
    /// Utility and its arguments.
    pub command: Vec<&'a str>,
}

/// Parse `time` arguments (without `argv[0]`).
pub fn parse_args<'a, I>(args: I) -> Result<Options<'a>, ParseError>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut portable = false;
    let mut command = Vec::new();
    for arg in args {
        if command.is_empty() {
            match arg {
                "-p" => {
                    portable = true;
                    continue;
                }
                other if other.starts_with('-') && other != "-" => {
                    return Err(ParseError::UnknownOption);
                }
                _ => {}
            }
        }
        command.push(arg);
    }
    if command.is_empty() {
        return Err(ParseError::MissingUtility);
    }
    Ok(Options { portable, command })
}

/// Format the three timing values in seconds.
pub fn format(real: f64, user: f64, sys: f64, portable: bool) -> String {
    let _ = portable;
    format!("real {real:.2}\nuser {user:.2}\nsys {sys:.2}\n")
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

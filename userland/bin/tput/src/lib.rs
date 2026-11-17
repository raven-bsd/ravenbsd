// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! ANSI capability subset for POSIX `tput`.
//!
//! The kernel/console contract is ANSI, so the table covers the capabilities a script can
//! reasonably use here. `cup` takes the POSIX `row column` parameters; numeric capabilities
//! return their value; unknown names are a typed failure, never a silent empty output.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};

/// One capability result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// Terminal bytes to write.
    Bytes(String),
    /// Numeric capability value.
    Number(u16),
}

/// Capability errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// No capability name.
    MissingCapability,
    /// The name is not in the ANSI subset.
    UnknownCapability,
    /// `cup` needs exactly two positive parameters.
    BadParameter,
    /// A parameter was supplied to a capability that takes none.
    TooManyParameters,
    /// Empty terminal type.
    UnknownTerm,
}

/// Resolve `name` for `term` with `parameters`.
pub fn capability(term: &str, name: &str, parameters: &[u16]) -> Result<Value, Error> {
    if term.is_empty() {
        return Err(Error::UnknownTerm);
    }
    if name.is_empty() {
        return Err(Error::MissingCapability);
    }
    let bytes = match name {
        "clear" => "\x1b[H\x1b[2J".to_string(),
        "bold" => "\x1b[1m".to_string(),
        "sgr0" => "\x1b[0m".to_string(),
        "smso" => "\x1b[7m".to_string(),
        "rmso" => "\x1b[27m".to_string(),
        "smul" => "\x1b[4m".to_string(),
        "rmul" => "\x1b[24m".to_string(),
        "el" => "\x1b[K".to_string(),
        "ed" => "\x1b[J".to_string(),
        "bel" => "\x07".to_string(),
        "cup" => {
            if parameters.len() != 2 || parameters.contains(&0) {
                return Err(Error::BadParameter);
            }
            format!("\x1b[{};{}H", parameters[0], parameters[1])
        }
        "cols" => return Ok(Value::Number(80)),
        "lines" => return Ok(Value::Number(24)),
        _ => return Err(Error::UnknownCapability),
    };
    if name != "cup" && !parameters.is_empty() {
        return Err(Error::TooManyParameters);
    }
    Ok(Value::Bytes(bytes))
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

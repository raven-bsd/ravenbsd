// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure defaults and argument handling for `adduser`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// The synopsis printed after `usage: adduser`.
pub const USAGE: &str = "[name]";
/// Default login shell for a new account.
pub const DEFAULT_SHELL: &str = "/bin/brush";

/// Why an `adduser` command line is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// `adduser` has no options.
    UnknownOption(char),
    /// More than one name was supplied.
    ExtraOperand,
}

/// Parse `adduser [name]`.
///
/// # Errors
/// [`UsageError::UnknownOption`] for any option and [`UsageError::ExtraOperand`] for a second name.
pub fn parse_args<'a>(
    args: impl IntoIterator<Item = &'a str>,
) -> Result<Option<&'a str>, UsageError> {
    let mut operands = Vec::new();
    let mut parsing = true;
    for argument in args {
        if parsing && argument == "--" {
            parsing = false;
            continue;
        }
        if parsing && argument != "-" && argument.len() > 1 && argument.starts_with('-') {
            return Err(UsageError::UnknownOption(
                argument.chars().nth(1).unwrap_or('-'),
            ));
        }
        operands.push(argument);
    }
    match operands.len() {
        0 => Ok(None),
        1 => Ok(operands.first().copied()),
        _ => Err(UsageError::ExtraOperand),
    }
}

/// The default home directory for a new account.
#[must_use]
pub fn default_home(name: &str) -> String {
    format!("/home/{name}")
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

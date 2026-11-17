// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `zcat` argument handling over the `.Z` codec.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

pub use compress::decode;

/// Parsed `zcat` operands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// Input files.
    pub files: Vec<String>,
}

/// Why an argument list is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// An unknown option was supplied.
    UnknownOption,
    /// No input file was supplied.
    MissingFile,
}

/// Parse `zcat file ...`.
///
/// # Errors
///
/// Returns [`UsageError`] for an option or no input file.
pub fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Options, UsageError> {
    let mut files = Vec::new();
    let mut options = true;
    for argument in args {
        if options && argument == "--" {
            options = false;
            continue;
        }
        if options && argument.starts_with('-') && argument.len() > 1 {
            return Err(UsageError::UnknownOption);
        }
        files.push(argument);
    }
    if files.is_empty() {
        return Err(UsageError::MissingFile);
    }
    Ok(Options { files })
}

/// The source path for an operand, adding `.Z` when needed.
#[must_use]
pub fn source_name(path: &str) -> String {
    if path.ends_with(".Z") {
        path.into()
    } else {
        alloc::format!("{path}.Z")
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

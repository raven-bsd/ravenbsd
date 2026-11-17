// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `uncompress` argument handling over the `.Z` codec.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

pub use compress::decode;

/// Parsed `uncompress` operands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// Write to stdout and keep input files.
    pub stdout: bool,
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

/// Parse `uncompress [-c] file ...`.
///
/// # Errors
///
/// Returns [`UsageError`] for an unknown option or no input file.
pub fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Options, UsageError> {
    let mut stdout = false;
    let mut files = Vec::new();
    let mut options = true;
    for argument in args {
        if options && argument == "--" {
            options = false;
            continue;
        }
        if options && argument.starts_with('-') && argument.len() > 1 {
            if argument == "-c" {
                stdout = true;
                continue;
            }
            return Err(UsageError::UnknownOption);
        }
        files.push(argument);
    }
    if files.is_empty() {
        return Err(UsageError::MissingFile);
    }
    Ok(Options { stdout, files })
}

/// The output name for an input path.
#[must_use]
pub fn output_name(path: &str) -> String {
    path.strip_suffix(".Z").unwrap_or(path).to_string()
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

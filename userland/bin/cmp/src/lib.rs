// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure comparison and argument handling for `cmp`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

/// Parsed command line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options<'a> {
    /// List every differing byte.
    pub list: bool,
    /// Suppress output and only return status.
    pub silent: bool,
    /// Input paths.
    pub operands: Vec<&'a str>,
}

/// Why an argument list is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// An unknown short option.
    UnknownOption(char),
}

/// Parse `cmp [-l|-s] file1 file2`.
pub fn parse<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Options<'a>, UsageError> {
    let mut options = Options::default();
    let mut parse_options = true;
    for argument in args {
        if parse_options && argument == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && argument.len() > 1 && argument.starts_with('-') {
            for option in argument.chars().skip(1) {
                match option {
                    'l' => options.list = true,
                    's' => options.silent = true,
                    other => return Err(UsageError::UnknownOption(other)),
                }
            }
            continue;
        }
        options.operands.push(argument);
    }
    Ok(options)
}

/// One differing byte position (zero-based).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Difference {
    /// Zero-based byte offset.
    pub offset: usize,
    /// Left byte, or `None` at end of file.
    pub left: Option<u8>,
    /// Right byte, or `None` at end of file.
    pub right: Option<u8>,
}

/// Return the differing byte positions for `-l`.
#[must_use]
pub fn differences(left: &[u8], right: &[u8]) -> Vec<Difference> {
    let mut result = Vec::new();
    let length = left.len().max(right.len());
    for offset in 0..length {
        let a = left.get(offset).copied();
        let b = right.get(offset).copied();
        if a != b {
            result.push(Difference {
                offset,
                left: a,
                right: b,
            });
        }
    }
    result
}

/// Render one `-l` line as `<offset> <left-octal> <right-octal>`.
#[must_use]
pub fn format_difference(difference: &Difference) -> String {
    alloc::format!(
        "{} {:o} {:o}",
        difference.offset.saturating_add(1),
        difference.left.unwrap_or(0),
        difference.right.unwrap_or(0)
    )
}

/// The first differing offset, or `None` when the inputs are identical.
#[must_use]
pub fn first_difference(left: &[u8], right: &[u8]) -> Option<usize> {
    let length = left.len().min(right.len());
    for offset in 0..length {
        if left.get(offset) != right.get(offset) {
            return Some(offset);
        }
    }
    if left.len() == right.len() {
        None
    } else {
        Some(length)
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

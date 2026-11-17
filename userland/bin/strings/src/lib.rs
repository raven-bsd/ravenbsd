// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure printable-run extraction for `strings`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

/// Parsed command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options<'a> {
    /// Minimum printable-run length.
    pub min: usize,
    /// Scan every file (the default; kept for POSIX option compatibility).
    pub all: bool,
    /// Input paths.
    pub operands: Vec<&'a str>,
}

impl<'a> Default for Options<'a> {
    fn default() -> Self {
        Self {
            min: 4,
            all: true,
            operands: Vec::new(),
        }
    }
}

/// Why an argument list is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// An unknown short option.
    UnknownOption(char),
    /// `-n` is missing its value.
    MissingValue,
    /// `-n` is not a decimal number.
    InvalidValue,
}

/// Parse `strings [-a] [-n number] [file ...]`.
pub fn parse<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Options<'a>, UsageError> {
    let mut options = Options {
        operands: Vec::new(),
        ..Options::default()
    };
    let mut args = args.into_iter();
    let mut parse_options = true;
    while let Some(argument) = args.next() {
        if parse_options && argument == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && argument == "-n" {
            let value = args.next().ok_or(UsageError::MissingValue)?;
            options.min = value.parse().map_err(|_| UsageError::InvalidValue)?;
            continue;
        }
        if parse_options && argument.len() > 1 && argument.starts_with('-') {
            for option in argument.chars().skip(1) {
                match option {
                    'a' => options.all = true,
                    other => return Err(UsageError::UnknownOption(other)),
                }
            }
            continue;
        }
        options.operands.push(argument);
    }
    Ok(options)
}

/// Extract printable ASCII runs (0x20..=0x7e) of at least `min` bytes.
#[must_use]
pub fn extract(data: &[u8], min: usize) -> Vec<String> {
    let mut result = Vec::new();
    let mut run = Vec::new();
    for byte in data.iter().copied().chain(core::iter::once(0)) {
        if (0x20..=0x7e).contains(&byte) {
            run.push(byte);
        } else {
            if run.len() >= min.max(1) {
                result.push(String::from_utf8_lossy(&run).into_owned());
            }
            run.clear();
        }
    }
    result
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

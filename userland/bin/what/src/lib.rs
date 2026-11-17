// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! SCCS `what` string extraction.
//!
//! `what` scans each file for the SCCS marker `@(#)` and prints the marker string up to a
//! terminator (`"`, `>`, newline or NUL). `-s` stops after the first string in each file.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

/// Usage line.
pub const USAGE: &str = "what [-s] file ...";

/// Argument errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// No file operand was given.
    MissingFile,
    /// An option other than `-s` was given.
    UnknownOption,
}

/// Parsed command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options<'a> {
    /// Stop after the first string in each file.
    pub first_only: bool,
    /// File operands.
    pub files: Vec<&'a str>,
}

/// Parse `what` arguments (without `argv[0]`).
pub fn parse_args<'a, I>(args: I) -> Result<Options<'a>, ParseError>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut first_only = false;
    let mut files = Vec::new();
    for arg in args {
        if arg == "-s" {
            first_only = true;
        } else if arg.starts_with('-') && arg != "-" {
            return Err(ParseError::UnknownOption);
        } else {
            files.push(arg);
        }
    }
    if files.is_empty() {
        return Err(ParseError::MissingFile);
    }
    Ok(Options { first_only, files })
}

/// Extract the SCCS identification strings from `data`.
pub fn extract(data: &[u8], first_only: bool) -> Vec<String> {
    let mut found = Vec::new();
    let mut index = 0usize;
    while index + 4 <= data.len() {
        if &data[index..index + 4] != b"@(#)" {
            index += 1;
            continue;
        }
        let start = index;
        let mut end = index + 4;
        while end < data.len() && !matches!(data[end], b'"' | b'>' | b'\n' | 0) {
            end += 1;
        }
        found.push(String::from_utf8_lossy(&data[start..end]).into_owned());
        if first_only {
            break;
        }
        index = end.max(index + 1);
    }
    found
}

/// Render the strings as `what` output lines.
pub fn render(strings: &[String]) -> String {
    let mut out = String::new();
    for string in strings {
        out.push('\t');
        out.push_str(string);
        out.push('\n');
    }
    out
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX `asa` carriage-control interpretation.
//!
//! The pure model maps the first byte of every input line to line-printer control characters.
//! The target wrapper supplies the file or standard-input bytes and writes the result to the
//! console.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::vec::Vec;

/// Why an argument list is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// An option was supplied; `asa` accepts only file operands.
    UnknownOption,
}

/// Parse `asa [file ...]`.
///
/// A lone `-` is an operand so the target wrapper can read standard input. `--` ends option
/// processing; every other argument beginning with `-` is rejected because POSIX `asa` has no
/// options.
pub fn parse<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Vec<&'a str>, UsageError> {
    let mut operands = Vec::new();
    let mut options = true;
    for argument in args {
        if options && argument == "--" {
            options = false;
            continue;
        }
        if options && argument.len() > 1 && argument.starts_with('-') {
            return Err(UsageError::UnknownOption);
        }
        operands.push(argument);
    }
    Ok(operands)
}

/// Interpret the carriage-control characters in `input` and return the printer stream.
///
/// The first byte of each line is the control character:
///
/// * a space, an empty line or any unrecognised byte advances one line;
/// * `0` advances two lines;
/// * `1` starts a new page with a form feed;
/// * `+` overstrikes the previous line with a carriage return.
///
/// The control byte is removed. The first line never starts with a blank line, and a non-empty
/// input always produces a trailing newline. An empty input produces no output.
pub fn interpret(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    if input.is_empty() {
        return out;
    }
    let mut start = 0usize;
    let mut first = true;
    while start < input.len() {
        let end = input[start..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(input.len(), |offset| start + offset);
        let line = &input[start..end];
        start = if end < input.len() { end + 1 } else { end };
        let control = line.first().copied().unwrap_or(b' ');
        let content = if line.is_empty() { &[][..] } else { &line[1..] };
        if first {
            match control {
                b'0' => out.push(b'\n'),
                b'1' => out.push(b'\x0c'),
                _ => {}
            }
            first = false;
        } else {
            match control {
                b'0' => out.extend_from_slice(b"\n\n"),
                b'1' => out.push(b'\x0c'),
                b'+' => out.push(b'\r'),
                _ => out.push(b'\n'),
            }
        }
        out.extend_from_slice(content);
    }
    out.push(b'\n');
    out
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

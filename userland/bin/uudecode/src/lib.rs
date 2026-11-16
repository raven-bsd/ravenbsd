// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX `uudecode` argument handling over the `uuencode` codec.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

pub use uuencode::{Decoded, Error as CodecError, decode};

/// Parsed `uudecode` operands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options<'a> {
    /// Explicit output path, overriding the encoded name.
    pub output: Option<&'a str>,
    /// Encoded input file; `None` means standard input.
    pub input: Option<&'a str>,
}

/// Why an argument list is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// An option other than `-o` was supplied.
    UnknownOption,
    /// `-o` has no following argument.
    MissingOutput,
    /// More than one input operand was supplied.
    ExtraOperand,
}

/// Parse `uudecode [-o output] [input]`.
///
/// # Errors
///
/// Returns [`UsageError`] for an unknown option, a missing `-o` value or more than
/// one input operand.
pub fn parse<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Options<'a>, UsageError> {
    let mut options = Options {
        output: None,
        input: None,
    };
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument {
            "-o" => {
                let Some(output) = args.next() else {
                    return Err(UsageError::MissingOutput);
                };
                options.output = Some(output);
            }
            "--" => {
                if let Some(input) = args.next() {
                    if options.input.is_some() {
                        return Err(UsageError::ExtraOperand);
                    }
                    options.input = Some(input);
                }
                break;
            }
            other if other.starts_with('-') && other != "-" => {
                return Err(UsageError::UnknownOption);
            }
            input => {
                if options.input.is_some() {
                    return Err(UsageError::ExtraOperand);
                }
                options.input = Some(input);
            }
        }
    }
    Ok(options)
}

/// Whether an encoded file name is safe to create in the current directory.
#[must_use]
pub fn safe_output_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains('/') && !name.contains('\\')
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

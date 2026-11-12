// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Small POSIX-style short-option parser used by all tools.

use alloc::vec::Vec;

/// Boolean short options understood by the file tools.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Flags {
    /// `-a`: include dot entries in directory listings.
    pub a: bool,
    /// `-l`: long directory listing.
    pub l: bool,
    /// `-p`: create missing parent directories.
    pub p: bool,
    /// `-r`: recurse.
    pub r: bool,
    /// `-f`: ignore missing operands.
    pub f: bool,
    /// `-h`: human-readable sizes where the tool supports it.
    pub h: bool,
    /// `-i`: interactive mode; parsed for compatibility, not implemented.
    pub i: bool,
}

/// The result of parsing one command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parsed<'a> {
    /// Selected flags.
    pub flags: Flags,
    /// Non-option operands in input order.
    pub operands: Vec<&'a str>,
}

/// Why an argument list is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// A short option not listed in the tool's allowed set.
    UnknownOption(char),
}

/// Parse short options and operands.
///
/// `allowed` contains the accepted option characters. A lone `-` is an operand (the tools do not
/// implement stdin). `--` ends option parsing; everything after it is an operand. Combined flags
/// such as `-lh` are accepted.
pub fn parse<'a>(
    args: impl IntoIterator<Item = &'a str>,
    allowed: &str,
) -> Result<Parsed<'a>, UsageError> {
    let mut flags = Flags::default();
    let mut operands = Vec::new();
    let mut options = true;
    for argument in args {
        if options && argument == "--" {
            options = false;
            continue;
        }
        if options && argument.len() > 1 && argument.starts_with('-') {
            for option in argument.chars().skip(1) {
                if !allowed.contains(option) {
                    return Err(UsageError::UnknownOption(option));
                }
                match option {
                    'a' => flags.a = true,
                    'l' => flags.l = true,
                    'p' => flags.p = true,
                    'r' => flags.r = true,
                    'f' => flags.f = true,
                    'h' => flags.h = true,
                    'i' => flags.i = true,
                    _ => return Err(UsageError::UnknownOption(option)),
                }
            }
            continue;
        }
        operands.push(argument);
    }
    Ok(Parsed { flags, operands })
}

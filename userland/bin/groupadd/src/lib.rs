// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure argument handling for `groupadd`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::vec::Vec;

/// The synopsis printed after `usage: groupadd`.
pub const USAGE: &str = "[-g gid] [-r] name";

/// Parsed `groupadd` options.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options<'a> {
    /// `-g`: explicit gid.
    pub gid: Option<u32>,
    /// `-r`: allocate a system gid.
    pub system: bool,
    /// The group name operand.
    pub name: &'a str,
}

/// Why a `groupadd` command line is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// An option that needs a value has none.
    MissingValue(char),
    /// An option is not in the synopsis.
    UnknownOption(char),
    /// A numeric option value is malformed.
    BadValue(char),
    /// No group name was supplied.
    MissingName,
    /// More than one group name was supplied.
    ExtraOperand,
}

/// Parse `groupadd [-g gid] [-r] name`.
///
/// # Errors
/// [`UsageError`] for an unknown option, a missing option value, a malformed gid, or a wrong
/// operand count.
pub fn parse_args<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Options<'a>, UsageError> {
    let args: Vec<&'a str> = args.into_iter().collect();
    let mut options = Options::default();
    let mut operands = Vec::new();
    let mut index = 0usize;
    let mut parsing = true;
    while let Some(argument) = args.get(index).copied() {
        index += 1;
        if parsing && argument == "--" {
            parsing = false;
            continue;
        }
        if !parsing || argument == "-" || !argument.starts_with('-') {
            operands.push(argument);
            continue;
        }
        let mut characters = argument.chars();
        let _dash = characters.next();
        let Some(flag) = characters.next() else {
            operands.push(argument);
            continue;
        };
        match flag {
            'r' if characters.next().is_none() => options.system = true,
            'g' => {
                let attached = argument.get(2..).unwrap_or("");
                let value = if attached.is_empty() {
                    let Some(value) = args.get(index).copied() else {
                        return Err(UsageError::MissingValue(flag));
                    };
                    index += 1;
                    value
                } else {
                    attached
                };
                options.gid = Some(
                    value
                        .parse::<u32>()
                        .map_err(|_| UsageError::BadValue('g'))?,
                );
            }
            other => return Err(UsageError::UnknownOption(other)),
        }
    }
    options.name = match operands.len() {
        0 => return Err(UsageError::MissingName),
        1 => operands.first().copied().unwrap_or(""),
        _ => return Err(UsageError::ExtraOperand),
    };
    Ok(options)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

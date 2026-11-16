// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure argument handling for `useradd`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::vec::Vec;

/// The synopsis printed after `usage: useradd`.
pub const USAGE: &str = "[-m] [-d home] [-s shell] [-u uid] [-g group] [-G list] [-c gecos] name";

/// Parsed `useradd` options.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options<'a> {
    /// `-m`: create the home directory.
    pub create_home: bool,
    /// `-d`: explicit home directory.
    pub home: Option<&'a str>,
    /// `-s`: login shell.
    pub shell: Option<&'a str>,
    /// `-u`: explicit uid.
    pub uid: Option<u32>,
    /// `-g`: existing primary group.
    pub group: Option<&'a str>,
    /// `-G`: supplementary groups.
    pub groups: Vec<&'a str>,
    /// `-c`: gecos field.
    pub gecos: Option<&'a str>,
    /// The login name operand.
    pub name: &'a str,
}

/// Why a `useradd` command line is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// An option that needs a value has none.
    MissingValue(char),
    /// An option is not in the synopsis.
    UnknownOption(char),
    /// A numeric option value is malformed.
    BadValue(char),
    /// No login name was supplied.
    MissingName,
    /// More than one login name was supplied.
    ExtraOperand,
}

/// Parse `useradd [-m] [-d home] [-s shell] [-u uid] [-g group] [-G list] [-c gecos] name`.
///
/// # Errors
/// [`UsageError`] for an unknown option, a missing option value, a malformed uid, or a wrong
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
            'm' if characters.next().is_none() => options.create_home = true,
            'd' | 's' | 'u' | 'g' | 'G' | 'c' => {
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
                match flag {
                    'd' => options.home = Some(value),
                    's' => options.shell = Some(value),
                    'u' => {
                        options.uid = Some(
                            value
                                .parse::<u32>()
                                .map_err(|_| UsageError::BadValue('u'))?,
                        )
                    }
                    'g' => options.group = Some(value),
                    'G' => {
                        options.groups = value.split(',').filter(|name| !name.is_empty()).collect()
                    }
                    'c' => options.gecos = Some(value),
                    _ => return Err(UsageError::UnknownOption(flag)),
                }
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

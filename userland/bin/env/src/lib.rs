// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure argument, environment and path handling for `env`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// The synopsis printed after `usage: env`.
pub const USAGE: &str = "[-i] [name=value ...] [command [argument ...]]";

/// A parsed `env` command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// Start from an empty environment.
    pub ignore: bool,
    /// `name=value` assignments before the command.
    pub assignments: Vec<(String, String)>,
    /// The command and its arguments, if any.
    pub command: Vec<String>,
}

/// Why an `env` command line is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// An unknown option.
    UnknownOption,
    /// A malformed assignment such as `=value`.
    BadAssignment,
}

/// Parse `env [-i] [name=value ...] [command [argument ...]]`.
pub fn parse_args<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Options, UsageError> {
    let mut ignore = false;
    let mut assignments = Vec::new();
    let mut command = Vec::new();
    let mut parsing = true;
    for argument in args {
        if parsing && argument == "--" {
            parsing = false;
            continue;
        }
        if parsing && (argument == "-i" || argument == "--ignore-environment") {
            ignore = true;
            continue;
        }
        if parsing && argument.starts_with('-') && argument != "-" && !argument.contains('=') {
            return Err(UsageError::UnknownOption);
        }
        if command.is_empty() && parsing {
            if let Some((name, value)) = argument.split_once('=') {
                if name.is_empty() {
                    return Err(UsageError::BadAssignment);
                }
                assignments.push((name.to_string(), value.to_string()));
                continue;
            }
            parsing = false;
        }
        command.push(argument.to_string());
    }
    Ok(Options {
        ignore,
        assignments,
        command,
    })
}

/// Merge assignments into a base environment.
///
/// The base is copied unless `ignore` is set. An assignment replaces the first existing entry
/// with the same name, otherwise it is appended in command-line order.
#[must_use]
pub fn merge_environment(
    base: &[String],
    ignore: bool,
    assignments: &[(String, String)],
) -> Vec<String> {
    let mut environment: Vec<String> = if ignore { Vec::new() } else { base.to_vec() };
    for (name, value) in assignments {
        let entry = alloc::format!("{name}={value}");
        if let Some(slot) = environment
            .iter_mut()
            .find(|item| item.split_once('=').is_some_and(|(key, _)| key == name))
        {
            *slot = entry;
        } else {
            environment.push(entry);
        }
    }
    environment
}

/// Return the value of `name` in `environment`, if present.
#[must_use]
pub fn value<'a>(environment: &'a [String], name: &str) -> Option<&'a str> {
    environment
        .iter()
        .find_map(|item| item.split_once('=').filter(|(key, _)| *key == name))
        .map(|(_, value)| value)
}

/// Candidate absolute paths for `command`.
///
/// A command containing `/` is used as written. Otherwise `PATH` is split on `:` and the command
/// is appended to each directory. An empty `PATH` falls back to `/bin:/sbin`.
#[must_use]
pub fn candidates(command: &str, environment: &[String]) -> Vec<String> {
    if command.contains('/') {
        return alloc::vec![command.to_string()];
    }
    let path = value(environment, "PATH").unwrap_or("/bin:/sbin");
    path.split(':')
        .filter(|directory| !directory.is_empty())
        .map(|directory| alloc::format!("{directory}/{command}"))
        .collect()
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

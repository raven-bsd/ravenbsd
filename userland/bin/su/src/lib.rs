// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure argument, environment and shell-plan handling for `su`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use raven_userdb::PasswdEntry;

/// The account used when the operand is omitted.
pub const DEFAULT_USER: &str = "root";
/// Fallback shell when the passwd record has none.
pub const DEFAULT_SHELL: &str = "/bin/brush";
/// The session path used by `su`.
pub const SESSION_PATH: &str = "/bin:/usr/bin:/sbin:/usr/sbin";
/// The synopsis printed after `usage: su`.
pub const USAGE: &str = "[-] [-c command] [user]";

/// Parsed `su` options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options<'a> {
    /// `-`: start a login-style session.
    pub login: bool,
    /// `-c`: run one command through the shell.
    pub command: Option<&'a str>,
    /// Target account.
    pub user: &'a str,
}

/// Why a `su` command line is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// `-c` has no value.
    MissingValue,
    /// An option is not in the synopsis.
    UnknownOption(char),
    /// More than one account was supplied.
    ExtraOperand,
}

/// Parse `su [-] [-c command] [user]`.
///
/// # Errors
/// [`UsageError`] for an unknown option, a missing `-c` value, or a wrong operand count.
pub fn parse_args<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Options<'a>, UsageError> {
    let args: Vec<&'a str> = args.into_iter().collect();
    let mut login = false;
    let mut command = None;
    let mut operands = Vec::new();
    let mut index = 0usize;
    let mut parsing = true;
    while let Some(argument) = args.get(index).copied() {
        index += 1;
        if parsing && argument == "--" {
            parsing = false;
            continue;
        }
        if parsing && argument == "-" {
            login = true;
            continue;
        }
        if parsing && argument.starts_with('-') && argument.len() > 1 {
            if argument == "-c" {
                let Some(value) = args.get(index).copied() else {
                    return Err(UsageError::MissingValue);
                };
                index += 1;
                command = Some(value);
                continue;
            }
            if let Some(value) = argument.strip_prefix("-c") {
                command = Some(value);
                continue;
            }
            return Err(UsageError::UnknownOption(
                argument.chars().nth(1).unwrap_or('-'),
            ));
        }
        operands.push(argument);
    }
    let user = match operands.len() {
        0 => DEFAULT_USER,
        1 => operands.first().copied().unwrap_or(DEFAULT_USER),
        _ => return Err(UsageError::ExtraOperand),
    };
    Ok(Options {
        login,
        command,
        user,
    })
}

/// Build the environment for the target shell.
#[must_use]
pub fn environment(user: &PasswdEntry, login: bool) -> Vec<String> {
    let home = if user.home.is_empty() {
        "/"
    } else {
        user.home.as_str()
    };
    let shell = if user.shell.is_empty() {
        DEFAULT_SHELL
    } else {
        user.shell.as_str()
    };
    let cwd = if login { home } else { "/" };
    alloc::vec![
        format!("HOME={home}"),
        format!("SHELL={shell}"),
        format!("USER={}", user.name),
        format!("LOGNAME={}", user.name),
        format!("PATH={SESSION_PATH}"),
        format!("PWD={cwd}"),
    ]
}

/// The arguments passed to the shell for this command.
#[must_use]
pub fn shell_arguments<'a>(options: &'a Options<'a>) -> Vec<&'a str> {
    match options.command {
        Some(command) => alloc::vec!["-c", command],
        None => Vec::new(),
    }
}

/// The shell path recorded for the target account.
#[must_use]
pub fn shell_path(user: &PasswdEntry) -> String {
    if user.shell.is_empty() {
        DEFAULT_SHELL.to_string()
    } else {
        user.shell.clone()
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

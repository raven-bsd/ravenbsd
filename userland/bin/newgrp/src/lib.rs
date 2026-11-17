// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure argument, group and shell-plan handling for `newgrp`.
//!
//! `newgrp` resolves the target group from the existing `/etc/group` database, requires the
//! caller to be root, the group's primary member, or an explicit member, installs the group with
//! the existing libc credential path, and starts the user's shell. A group password is not
//! prompted for in this slice: a non-member gets the documented permission error.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use raven_userdb::{GroupEntry, PasswdEntry};

/// Fallback shell when neither `$SHELL` nor the passwd record names one.
pub const DEFAULT_SHELL: &str = "/bin/brush";
/// The session path used for a login-style shell.
pub const SESSION_PATH: &str = "/bin:/usr/bin:/sbin:/usr/sbin";
/// The synopsis printed after `usage: newgrp`.
pub const USAGE: &str = "[-l] [group]";
/// Diagnostic for a non-member without a usable group password.
pub const NO_PASSWORD: &str = "permission denied: group password is not supported";

/// Parsed `newgrp` options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options<'a> {
    /// `-l`: start a login-style shell.
    pub login: bool,
    /// Target group name or numeric gid; `None` means the caller's login group.
    pub group: Option<&'a str>,
}

/// Why a `newgrp` command line is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// An option is not in the synopsis.
    UnknownOption(char),
    /// More than one group operand was supplied.
    ExtraOperand,
}

/// Parse `newgrp [-l] [group]`.
///
/// # Errors
/// [`UsageError`] for an unknown option or more than one operand.
pub fn parse_args<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Options<'a>, UsageError> {
    let mut login = false;
    let mut operand = None;
    let mut parsing = true;
    for argument in args {
        if parsing && argument == "--" {
            parsing = false;
            continue;
        }
        if parsing && (argument == "-l" || argument == "-") {
            login = true;
            continue;
        }
        if parsing && argument.starts_with('-') && argument.len() > 1 {
            return Err(UsageError::UnknownOption(
                argument.chars().nth(1).unwrap_or('-'),
            ));
        }
        if operand.is_some() {
            return Err(UsageError::ExtraOperand);
        }
        operand = Some(argument);
    }
    Ok(Options {
        login,
        group: operand,
    })
}

/// Find a group by name, or by numeric gid when the operand is all digits.
#[must_use]
pub fn find_group<'a>(groups: &'a [GroupEntry], spec: &str) -> Option<&'a GroupEntry> {
    if !spec.is_empty() && spec.bytes().all(|byte| byte.is_ascii_digit()) {
        let gid = spec.parse::<u32>().ok()?;
        return groups.iter().find(|group| group.gid == gid);
    }
    groups.iter().find(|group| group.name == spec)
}

/// One membership decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// The caller may switch to the group.
    Allowed,
    /// The caller may not switch without a password prompt.
    Denied,
}

/// Whether `user` may switch to `group`.
///
/// Root may switch to any group. The user's primary group and an explicit member entry are
/// allowed; everything else is denied because this slice does not prompt for a group password.
#[must_use]
pub fn membership(group: &GroupEntry, user: &PasswdEntry, root: bool) -> Access {
    if root || user.gid == group.gid || group.members.iter().any(|name| name == &user.name) {
        Access::Allowed
    } else {
        Access::Denied
    }
}

/// The shell invocation planned for the new group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellPlan {
    /// Shell path.
    pub shell: String,
    /// Arguments; the first element is `argv[0]` when non-empty.
    pub args: Vec<String>,
    /// Environment entries; empty means inherit the current environment.
    pub env: Vec<String>,
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Build the shell plan.
///
/// `$SHELL` wins when set, then the passwd shell, then [`DEFAULT_SHELL`]. `-l` makes `argv[0]`
/// the dashed shell name and supplies the login-style environment.
#[must_use]
pub fn shell_plan(user: &PasswdEntry, env_shell: Option<&str>, login: bool) -> ShellPlan {
    let shell = env_shell
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            if user.shell.is_empty() {
                None
            } else {
                Some(user.shell.clone())
            }
        })
        .unwrap_or_else(|| DEFAULT_SHELL.to_string());
    if !login {
        return ShellPlan {
            shell,
            args: Vec::new(),
            env: Vec::new(),
        };
    }
    let home = if user.home.is_empty() {
        "/"
    } else {
        user.home.as_str()
    };
    ShellPlan {
        args: alloc::vec![format!("-{}", basename(&shell))],
        env: alloc::vec![
            format!("HOME={home}"),
            format!("SHELL={shell}"),
            format!("USER={}", user.name),
            format!("LOGNAME={}", user.name),
            format!("PATH={SESSION_PATH}"),
            format!("PWD={home}"),
        ],
        shell,
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

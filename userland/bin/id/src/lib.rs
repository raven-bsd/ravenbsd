// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure argument and line formatting for `id`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use raven_userdb::{Credential, Database, DbError};

/// The synopsis printed after `usage: id`.
pub const USAGE: &str = "[-u] [-g] [-G] [-n] [user]";

/// Which identity component to print.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Selection {
    /// The full `uid=/gid=/groups=` line.
    #[default]
    All,
    /// Effective uid.
    User,
    /// Effective gid.
    Group,
    /// All group ids.
    Groups,
}

/// Parsed `id` options.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options<'a> {
    /// Selected component.
    pub selection: Selection,
    /// Print names instead of numbers.
    pub names: bool,
    /// Account operand; `None` means the caller.
    pub user: Option<&'a str>,
}

/// Why an `id` command line is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// An option is not in the synopsis.
    UnknownOption(char),
    /// More than one of `-u`, `-g` and `-G`.
    Conflict,
    /// `-n` was used without a component selector.
    NamesWithoutSelection,
    /// More than one account was supplied.
    ExtraOperand,
}

/// Parse `id [-u] [-g] [-G] [-n] [user]`.
///
/// # Errors
/// [`UsageError`] for an unknown option, conflicting selectors, `-n` alone, or extra operands.
pub fn parse_args<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Options<'a>, UsageError> {
    let args: Vec<&'a str> = args.into_iter().collect();
    let mut options = Options::default();
    let mut operands = Vec::new();
    let mut parsing = true;
    let mut selectors = 0usize;
    for argument in args {
        if parsing && argument == "--" {
            parsing = false;
            continue;
        }
        if !parsing || argument == "-" || !argument.starts_with('-') {
            operands.push(argument);
            continue;
        }
        for flag in argument.chars().skip(1) {
            match flag {
                'u' => {
                    selectors += 1;
                    options.selection = Selection::User;
                }
                'g' => {
                    selectors += 1;
                    options.selection = Selection::Group;
                }
                'G' => {
                    selectors += 1;
                    options.selection = Selection::Groups;
                }
                'n' => options.names = true,
                other => return Err(UsageError::UnknownOption(other)),
            }
        }
    }
    if selectors > 1 {
        return Err(UsageError::Conflict);
    }
    if options.names && options.selection == Selection::All {
        return Err(UsageError::NamesWithoutSelection);
    }
    options.user = match operands.len() {
        0 => None,
        1 => operands.first().copied(),
        _ => return Err(UsageError::ExtraOperand),
    };
    Ok(options)
}

/// Find the credential for an account operand, or the root identity for the current process.
///
/// # Errors
/// [`DbError::UnknownUser`] for an unknown operand and [`DbError::TooManyGroups`] when the account
/// exceeds the kernel limit.
pub fn credential_for(database: &Database, user: Option<&str>) -> Result<Credential, DbError> {
    match user {
        Some(name) => {
            let record = database
                .user(name)
                .or_else(|| {
                    name.parse::<u32>()
                        .ok()
                        .and_then(|uid| database.user_by_uid(uid))
                })
                .ok_or(DbError::UnknownUser)?;
            Credential::for_user(database, record)
        }
        None => match database.user_by_uid(0) {
            Some(root) => Credential::for_user(database, root),
            None => Ok(Credential::root()),
        },
    }
}

/// Format the selected identity.
#[must_use]
pub fn format(database: &Database, credential: &Credential, options: &Options<'_>) -> String {
    match options.selection {
        Selection::All => {
            let mut text = format!("uid={}", user_label(database, credential.uid));
            if credential.euid != credential.uid {
                text.push_str(&format!(" euid={}", user_label(database, credential.euid)));
            }
            text.push_str(&format!(" gid={}", group_label(database, credential.gid)));
            if credential.egid != credential.gid {
                text.push_str(&format!(" egid={}", group_label(database, credential.egid)));
            }
            let groups: Vec<String> = credential
                .groups
                .iter()
                .map(|gid| group_label(database, *gid))
                .collect();
            text.push_str(&format!(" groups={}", groups.join(",")));
            text
        }
        Selection::User if options.names => user_name(database, credential.euid),
        Selection::User => credential.euid.to_string(),
        Selection::Group if options.names => group_name(database, credential.egid),
        Selection::Group => credential.egid.to_string(),
        Selection::Groups if options.names => credential
            .groups
            .iter()
            .map(|gid| group_name(database, *gid))
            .collect::<Vec<String>>()
            .join(","),
        Selection::Groups => credential
            .groups
            .iter()
            .map(u32::to_string)
            .collect::<Vec<String>>()
            .join(","),
    }
}

fn user_name(database: &Database, uid: u32) -> String {
    database
        .user_by_uid(uid)
        .map(|user| user.name.clone())
        .unwrap_or_else(|| uid.to_string())
}

fn group_name(database: &Database, gid: u32) -> String {
    database
        .group_by_gid(gid)
        .map(|group| group.name.clone())
        .unwrap_or_else(|| gid.to_string())
}

fn user_label(database: &Database, uid: u32) -> String {
    match database.user_by_uid(uid) {
        Some(user) => format!("{uid}({})", user.name),
        None => uid.to_string(),
    }
}

fn group_label(database: &Database, gid: u32) -> String {
    match database.group_by_gid(gid) {
        Some(group) => format!("{gid}({})", group.name),
        None => gid.to_string(),
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

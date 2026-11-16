// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure argument and password checks for `passwd`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::vec::Vec;

/// The synopsis printed after `usage: passwd`.
pub const USAGE: &str = "[-l|-u] [name]";

/// What `passwd` should do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Action {
    /// Prompt for and install a new password.
    #[default]
    Change,
    /// Prefix the shadow hash with a lock marker.
    Lock,
    /// Remove one lock marker.
    Unlock,
}

/// Parsed `passwd` options.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options<'a> {
    /// Selected action.
    pub action: Action,
    /// Explicit account, or `None` for the caller.
    pub user: Option<&'a str>,
}

/// Why a `passwd` command line is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// An option is not in the synopsis.
    UnknownOption(char),
    /// `-l` and `-u` were both supplied.
    Conflict,
    /// More than one account was supplied.
    ExtraOperand,
}

/// Why a new password is not acceptable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasswordError {
    /// The password is empty.
    Empty,
    /// The password is shorter than the policy minimum.
    TooShort,
    /// The password contains a NUL byte.
    Nul,
    /// The password contains a line break.
    Newline,
}

impl PasswordError {
    /// A short diagnostic for the tool.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::Empty => "empty password",
            Self::TooShort => "password is too short",
            Self::Nul => "password contains a NUL byte",
            Self::Newline => "password contains a line break",
        }
    }
}

/// The minimum length for a new password, following the 0.2.0 root rule.
///
/// Root may set a one-character recovery password on the root account. Every other combination
/// requires at least eight characters.
#[must_use]
pub const fn minimum_length(caller_uid: u32, target_uid: u32) -> usize {
    if caller_uid == 0 && target_uid == 0 {
        1
    } else {
        8
    }
}

/// Check a new password against the 0.2.0 rules.
///
/// # Errors
/// [`PasswordError`] when the password is empty, too short, or contains NUL/newline.
pub fn validate_password(
    password: &str,
    caller_uid: u32,
    target_uid: u32,
) -> Result<(), PasswordError> {
    if password.is_empty() {
        return Err(PasswordError::Empty);
    }
    if password.contains('\0') {
        return Err(PasswordError::Nul);
    }
    if password.contains('\n') || password.contains('\r') {
        return Err(PasswordError::Newline);
    }
    if password.chars().count() < minimum_length(caller_uid, target_uid) {
        return Err(PasswordError::TooShort);
    }
    Ok(())
}

/// Convert seconds since the epoch to whole days, clamped at zero.
#[must_use]
pub fn days_since_epoch(seconds: i64) -> i64 {
    seconds.div_euclid(86_400).max(0)
}

/// A non-root real uid must prove the old password before changing its own.
#[must_use]
pub const fn requires_old_password(real_uid: u32) -> bool {
    real_uid != 0
}

/// Root may target any account; a non-root caller may only target its own uid.
#[must_use]
pub const fn target_allowed(real_uid: u32, target_uid: u32) -> bool {
    real_uid == 0 || real_uid == target_uid
}

/// Verify an old password against the target hash with `raven-auth`.
///
/// A missing, empty or locked hash never authenticates.
#[must_use]
pub fn verify_old_password(hash: &str, password: &str) -> bool {
    if hash.is_empty() || hash.starts_with('!') || hash.starts_with('*') {
        return false;
    }
    raven_auth::verify_password(password, hash)
}

/// Parse `passwd [-l|-u] [name]`.
///
/// # Errors
/// [`UsageError`] for an unknown option, a conflicting action, or more than one operand.
pub fn parse_args<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Options<'a>, UsageError> {
    let args: Vec<&'a str> = args.into_iter().collect();
    let mut options = Options::default();
    let mut operands = Vec::new();
    let mut parsing = true;
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
                'l' if options.action == Action::Change => options.action = Action::Lock,
                'l' if options.action == Action::Lock => {}
                'u' if options.action == Action::Change => options.action = Action::Unlock,
                'u' if options.action == Action::Unlock => {}
                'l' | 'u' => return Err(UsageError::Conflict),
                other => return Err(UsageError::UnknownOption(other)),
            }
        }
    }
    options.user = match operands.len() {
        0 => None,
        1 => operands.first().copied(),
        _ => return Err(UsageError::ExtraOperand),
    };
    Ok(options)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

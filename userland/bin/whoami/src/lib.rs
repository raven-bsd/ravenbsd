// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure argument and name handling for `whoami`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use raven_userdb::Database;

/// The synopsis printed after `usage: whoami`.
pub const USAGE: &str = "";

/// Why a `whoami` command line is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// `whoami` takes no operands.
    ExtraOperand,
}

/// Parse `whoami`.
///
/// # Errors
/// [`UsageError::ExtraOperand`] when any argument is supplied.
pub fn parse_args<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<(), UsageError> {
    let operands: Vec<&'a str> = args.into_iter().collect();
    if operands.is_empty() {
        Ok(())
    } else {
        Err(UsageError::ExtraOperand)
    }
}

/// The name for `uid`, or the numeric uid when the account database has no entry.
#[must_use]
pub fn name_for_uid(database: &Database, uid: u32) -> String {
    database
        .user_by_uid(uid)
        .map(|user| user.name.clone())
        .unwrap_or_else(|| uid.to_string())
}

/// True when the account database knows `uid`.
#[must_use]
pub fn known_uid(database: &Database, uid: u32) -> bool {
    database.user_by_uid(uid).is_some()
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Auth error classes.

/// A malformed or unsupported auth record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthError {
    /// The shadow line does not have exactly the nine colon-separated fields.
    InvalidRecord,
    /// A numeric shadow field is not an integer.
    InvalidNumber,
    /// The password hash could not be produced or the record is locked.
    Hash,
    /// The user is not the root account.
    NotRoot,
}

impl core::fmt::Display for AuthError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRecord => "invalid shadow record",
            Self::InvalidNumber => "invalid numeric shadow field",
            Self::Hash => "invalid password hash",
            Self::NotRoot => "not the root account",
        })
    }
}

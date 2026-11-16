// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Error types shared by the parser, the validator and the transaction.

use core::fmt;

use raven_abi::errno::Errno;

/// One of the three account files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DbFile {
    /// `/etc/passwd`.
    Passwd,
    /// `/etc/group`.
    Group,
    /// `/etc/shadow`.
    Shadow,
}

impl DbFile {
    /// The file name used in diagnostics.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Passwd => "passwd",
            Self::Group => "group",
            Self::Shadow => "shadow",
        }
    }
}

/// A user-database failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DbError {
    /// A record does not have the required number of colon-separated fields.
    FieldCount {
        /// Which file was being parsed.
        file: DbFile,
        /// One-based line number.
        line: usize,
        /// Required field count.
        expected: usize,
        /// Observed field count.
        found: usize,
    },
    /// A field failed validation.
    InvalidField {
        /// Which file was being parsed.
        file: DbFile,
        /// One-based line number.
        line: usize,
        /// The field name for the diagnostic.
        field: &'static str,
    },
    /// A name or id is duplicated.
    Duplicate {
        /// Which file was being parsed.
        file: DbFile,
        /// One-based line number of the duplicate.
        line: usize,
    },
    /// A requested account name does not exist.
    UnknownUser,
    /// A requested group name does not exist.
    UnknownGroup,
    /// No id is available in the requested range.
    NoFreeId,
    /// The account would exceed the kernel's supplementary-group limit.
    TooManyGroups,
    /// A name, path or gecos string is malformed.
    InvalidName,
    /// A shadow hash is not a `$6$` record or a lock marker.
    InvalidHash,
    /// Another process holds the account-database lock.
    Busy,
    /// The filesystem operation failed.
    Io(Errno),
    /// A partial commit could not be rolled back.
    CommitFailed,
}

impl fmt::Display for DbError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FieldCount {
                file,
                line,
                expected,
                found,
            } => write!(
                formatter,
                "{}: line {line}: expected {expected} fields, got {found}",
                file.name()
            ),
            Self::InvalidField { file, line, field } => {
                write!(formatter, "{}: line {line}: invalid {field}", file.name())
            }
            Self::Duplicate { file, line } => {
                write!(
                    formatter,
                    "{}: line {line}: duplicate name or id",
                    file.name()
                )
            }
            Self::UnknownUser => formatter.write_str("unknown user"),
            Self::UnknownGroup => formatter.write_str("unknown group"),
            Self::NoFreeId => formatter.write_str("no free id in range"),
            Self::TooManyGroups => formatter.write_str("too many supplementary groups"),
            Self::InvalidName => formatter.write_str("invalid account name"),
            Self::InvalidHash => formatter.write_str("invalid password hash"),
            Self::Busy => formatter.write_str("account database is locked"),
            Self::Io(error) => formatter.write_str(error.description()),
            Self::CommitFailed => formatter.write_str("account database update failed"),
        }
    }
}

#[cfg(test)]
#[path = "error_ut.rs"]
mod tests;

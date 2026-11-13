// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Typed failures carrying the process exit code (ticket C35, section 4.4).
//!
//! Ordinary errors are usage, I/O or recipe errors and exit with code 2. A licence, donor-pin or
//! export gate refusal exits with code 1; a missing or unusable host tool exits with code 77 so the
//! caller can report the run as skipped.

use std::fmt;

/// A failure with an explicit exit code.
#[derive(Debug)]
pub(crate) struct Failure {
    /// Process exit code.
    pub(crate) code: u8,
    /// Message printed by the binary (no `libc-oracle:` prefix).
    pub(crate) message: String,
}

impl Failure {
    /// A gate refusal (licence, donor pin, export mismatch): exit 1.
    pub(crate) fn refused(message: impl Into<String>) -> anyhow::Error {
        anyhow::Error::new(Self {
            code: 1,
            message: message.into(),
        })
    }

    /// A missing or unusable host tool: exit 77 with the `skipped:` prefix.
    pub(crate) fn skipped(tool: &str) -> anyhow::Error {
        Self::skipped_message(&format!("{tool} not found"))
    }

    /// Any other skip (unsupported host, missing capability): exit 77.
    pub(crate) fn skipped_message(message: &str) -> anyhow::Error {
        anyhow::Error::new(Self {
            code: 77,
            message: format!("skipped: {message}"),
        })
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Failure {}

/// Exit code for an error returned by [`crate::run`].
///
/// A typed `Failure` keeps its code; every other error is a usage, I/O or recipe error (2).
#[must_use]
pub fn exit_code(error: &anyhow::Error) -> u8 {
    error
        .downcast_ref::<Failure>()
        .map_or(2, |failure| failure.code)
}

#[cfg(test)]
#[path = "failure_ut.rs"]
mod tests;

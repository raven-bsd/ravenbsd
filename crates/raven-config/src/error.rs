// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Configuration errors with file, key and line where known.

use std::fmt;
use std::path::PathBuf;

/// One strict configuration failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigError {
    /// The file being read; empty for a compiled default.
    pub file: PathBuf,
    /// The dotted key, when the failure is tied to one.
    pub key: Option<String>,
    /// The 1-based line, when the parser provided one.
    pub line: Option<usize>,
    /// The human-readable detail.
    pub message: String,
}

impl ConfigError {
    /// Build an error tied to a file.
    #[must_use]
    pub fn file(file: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            key: None,
            line: None,
            message: message.into(),
        }
    }

    /// Add a dotted key.
    #[must_use]
    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Add a 1-based line.
    #[must_use]
    pub fn with_line(mut self, line: usize) -> Self {
        self.line = Some(line);
        self
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.file.as_os_str().is_empty() {
            write!(formatter, "{}", self.file.display())?;
            if let Some(line) = self.line {
                write!(formatter, ":{line}")?;
            }
            write!(formatter, ": ")?;
        }
        if let Some(key) = &self.key {
            write!(formatter, "`{key}`: ")?;
        }
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ConfigError {}

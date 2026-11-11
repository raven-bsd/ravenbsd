// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Result and option types shared by the library and the CLI.

use std::path::PathBuf;

use serde::Serialize;

/// A single rule violation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Violation {
    /// Repository-relative path, package name, or manifest the violation belongs to.
    pub path: String,
    /// Stable rule identifier such as `spdx-missing`.
    pub rule: String,
    /// Human-readable explanation.
    pub message: String,
}

impl Violation {
    pub(crate) fn new(path: impl Into<String>, rule: &str, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            rule: rule.to_string(),
            message: message.into(),
        }
    }
}

/// Options for [`crate::check`].
#[derive(Debug, Clone)]
pub struct CheckOptions {
    /// Repository root to inspect.
    pub root: PathBuf,
    /// Optional donors lock file. When `None`, `RAVEN_DONORS_LOCK` or the nearest
    /// `ravenbsd-resources/donors.lock` found from the root upwards is used.
    pub donors_lock: Option<PathBuf>,
    /// Treat a dependency scan that cannot run as a violation instead of a warning.
    ///
    /// The gate (`make check`) sets this; the plain CLI does not, so a missing cargo cache still
    /// produces a warning there.
    pub strict: bool,
}

impl CheckOptions {
    /// Create options for `root` with the default donors lock resolution and warnings for a
    /// broken dependency scan.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            donors_lock: None,
            strict: false,
        }
    }
}

/// Result of a [`crate::check`] run.
#[derive(Debug, Clone, Serialize)]
pub struct CheckReport {
    /// Every rule violation found, sorted by path and rule.
    pub violations: Vec<Violation>,
    /// Non-fatal problems, for example a missing donors lock or offline cargo failure.
    pub warnings: Vec<String>,
    /// Number of tracked files examined after applying `.provenance-ignore`.
    pub files_checked: usize,
}

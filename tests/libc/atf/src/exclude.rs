// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `excluded.toml`: manual and oracle exclusions (ticket C71, 4.5/4.7).
//!
//! Every entry has a non-empty reason. Manual exclusions count against the G1 cap of 250; oracle
//! exclusions are written by the runner when the host oracle itself fails (16 3.B.2.3).

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::glob_match;

/// The exclusion table.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ExcludeFile {
    /// The rows.
    #[serde(default, rename = "exclude")]
    pub entries: Vec<Exclude>,
}

/// One exclusion.
#[derive(Clone, Debug, Deserialize)]
pub struct Exclude {
    /// Suite name.
    pub suite: String,
    /// Program name; `*` and `?` are globs.
    pub program: String,
    /// Optional case ident glob.
    #[serde(default)]
    pub case: Option<String>,
    /// `manual` (counts toward the cap) or `oracle` (written after an oracle failure).
    pub kind: String,
    /// Mandatory one-line reason.
    pub reason: String,
    /// Optional owning ticket.
    #[serde(default)]
    pub ticket: Option<String>,
}

impl ExcludeFile {
    /// Load and validate `excluded.toml`; a missing file is an empty table.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.is_file() {
            return Ok(Self::default());
        }
        let text =
            fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
        let table: Self =
            toml::from_str(&text).with_context(|| format!("cannot parse {}", path.display()))?;
        table.validate()?;
        Ok(table)
    }

    /// Validate reason, kind and glob characters.
    pub fn validate(&self) -> Result<()> {
        for entry in &self.entries {
            if entry.reason.trim().is_empty() {
                bail!(
                    "excluded.toml: {}:{} has an empty reason",
                    entry.suite,
                    entry.program
                );
            }
            if !matches!(entry.kind.as_str(), "manual" | "oracle") {
                bail!(
                    "excluded.toml: {}:{} has kind `{}` (expected manual or oracle)",
                    entry.suite,
                    entry.program,
                    entry.kind
                );
            }
            for pattern in std::iter::once(&entry.program).chain(entry.case.iter()) {
                if !pattern
                    .chars()
                    .all(|character| character.is_ascii() || character == '*' || character == '?')
                {
                    bail!("excluded.toml: non-ASCII glob `{pattern}`");
                }
            }
        }
        Ok(())
    }

    /// The first matching exclusion.
    #[must_use]
    pub fn matched(&self, suite: &str, program: &str, case: Option<&str>) -> Option<&Exclude> {
        self.entries.iter().find(|entry| {
            entry.suite == suite
                && glob_match(&entry.program, program)
                && entry
                    .case
                    .as_deref()
                    .is_none_or(|pattern| case.is_some_and(|name| glob_match(pattern, name)))
        })
    }

    /// Number of manual exclusions, the G1 cap input.
    #[must_use]
    pub fn manual_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.kind == "manual")
            .count()
    }
}

#[cfg(test)]
#[path = "exclude_ut.rs"]
mod tests;

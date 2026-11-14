// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `owners.toml`: map a program path to the ticket that owns the code under test.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// The owner table.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Owners {
    /// The rows.
    #[serde(default, rename = "owner")]
    pub entries: Vec<Owner>,
}

/// One owner row.
#[derive(Clone, Debug, Deserialize)]
pub struct Owner {
    /// `suite:path/` prefix; the longest match wins.
    pub prefix: String,
    /// Owning ticket (`C40`).
    pub ticket: String,
    /// Area description.
    pub area: String,
}

impl Owners {
    /// Load and validate `owners.toml`.
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
        let owners: Self =
            toml::from_str(&text).with_context(|| format!("cannot parse {}", path.display()))?;
        owners.validate()?;
        Ok(owners)
    }

    /// Validate ticket names and prefix shape.
    pub fn validate(&self) -> Result<()> {
        for owner in &self.entries {
            if !owner.ticket.starts_with('C') || owner.ticket.len() < 2 {
                bail!(
                    "owners.toml: prefix `{}` has ticket `{}`",
                    owner.prefix,
                    owner.ticket
                );
            }
            if !owner.prefix.contains(':') {
                bail!(
                    "owners.toml: prefix `{}` must be `suite:path/`",
                    owner.prefix
                );
            }
            if owner.area.trim().is_empty() {
                bail!("owners.toml: prefix `{}` has an empty area", owner.prefix);
            }
        }
        Ok(())
    }

    /// Ticket for a program, or `-` when no row matches.
    #[must_use]
    pub fn ticket(&self, suite: &str, program: &str) -> String {
        let key = format!("{suite}:{program}");
        let mut best: Option<&Owner> = None;
        for owner in &self.entries {
            if key.starts_with(&owner.prefix)
                && best.is_none_or(|current| owner.prefix.len() > current.prefix.len())
            {
                best = Some(owner);
            }
        }
        best.map_or_else(|| "-".to_string(), |owner| owner.ticket.clone())
    }

    /// Top-level directories of a suite that no row covers.
    #[must_use]
    pub fn unmapped(&self, suite: &str, programs: &[String]) -> Vec<String> {
        let mut directories = BTreeSet::new();
        for program in programs {
            let directory = program.split('/').next().unwrap_or("");
            if !directory.is_empty() && !directory.contains('.') {
                directories.insert(directory.to_string());
            }
        }
        directories
            .into_iter()
            .filter(|directory| {
                let key = format!("{suite}:{directory}/");
                !self
                    .entries
                    .iter()
                    .any(|owner| key.starts_with(&owner.prefix) || owner.prefix.starts_with(&key))
            })
            .collect()
    }
}

/// A failing row must have an owner.
#[must_use]
pub fn fail_without_owner(ticket: &str) -> bool {
    ticket.is_empty() || ticket == "-"
}

#[cfg(test)]
#[path = "owners_ut.rs"]
mod tests;

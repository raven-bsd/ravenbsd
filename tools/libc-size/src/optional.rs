// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The reviewer-owned optional-personality allowlist for R-26.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

/// The optional-symbol table.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct OptionalSymbols {
    /// Schema version.
    pub schema: u32,
    /// Optional server-only names with a reason.
    #[serde(default, rename = "symbol")]
    pub symbols: Vec<OptionalSymbol>,
}

/// One optional server-only symbol.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct OptionalSymbol {
    /// Public name.
    pub name: String,
    /// Why tiny may omit it.
    pub reason: String,
}

impl OptionalSymbols {
    /// Read the allowlist from `tests/libc/size/optional-symbols.toml`.
    ///
    /// A missing file means no optional names are allowed, so the set-symbols check fails on
    /// every server-only name. A malformed file is an error.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.is_file() {
            return Ok(Self {
                schema: 1,
                symbols: Vec::new(),
            });
        }
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        let table: Self =
            toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
        for symbol in &table.symbols {
            if symbol.name.is_empty() || symbol.reason.trim().is_empty() {
                anyhow::bail!(
                    "{}: every optional symbol needs a name and a reason",
                    path.display()
                );
            }
        }
        Ok(table)
    }

    /// The allowed names.
    #[must_use]
    pub fn names(&self) -> BTreeSet<&str> {
        self.symbols
            .iter()
            .map(|symbol| symbol.name.as_str())
            .collect()
    }

    /// Remove every allowlisted name from a server-only set.
    #[must_use]
    pub fn filter_server_only(&self, names: Vec<String>) -> Vec<String> {
        let allowed = self.names();
        names
            .into_iter()
            .filter(|name| !allowed.contains(name.as_str()))
            .collect()
    }
}

/// Whether a symbol-set row is a failure: a tiny-only name is always a defect, and a
/// server-only name is a defect unless it was removed by [`OptionalSymbols::filter_server_only`].
#[must_use]
pub fn set_symbols_fail(tiny_only: &[String], server_only: &[String]) -> bool {
    !tiny_only.is_empty() || !server_only.is_empty()
}

#[cfg(test)]
#[path = "optional_ut.rs"]
mod tests;

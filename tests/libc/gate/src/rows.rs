// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `rows.toml` model and validation.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// One check described by `rows.toml`.
#[derive(Clone, Debug, Deserialize)]
pub struct CheckSpec {
    /// Check id.
    pub id: String,
    /// Row number, 1 through 6.
    pub row: u8,
    /// Human title.
    pub title: String,
    /// Whether the check gates the row.
    #[serde(default = "default_required")]
    pub required: bool,
    /// `argv`, `read` or `info`.
    #[serde(default = "default_kind")]
    pub kind: String,
    /// Argument vector for `argv` checks.
    #[serde(default)]
    pub argv: Vec<String>,
    /// Child argument vector for an expanding check; `{family}`, `{crate}` or `{case}` is replaced.
    #[serde(default)]
    pub child_argv: Vec<String>,
    /// Optional preflight command; a non-zero exit makes the check BLOCKED.
    #[serde(default)]
    pub preflight: Vec<String>,
    /// Exit-code pass rule.
    #[serde(default)]
    pub pass: PassRule,
    /// Conditions that make the check `BLOCKED` without running it.
    #[serde(default)]
    pub blocked_on: BlockedOn,
    /// Evidence path, repository-relative.
    #[serde(default)]
    pub evidence: String,
    /// Source citation.
    #[serde(default)]
    pub source: String,
    /// Reader name for `read` checks (`soak`, `qtest`, `hardened-malloc`, `entropy-weak`, `man`).
    #[serde(default)]
    pub read: Option<String>,
    /// Input file for a `read` check.
    #[serde(default)]
    pub file: Option<String>,
    /// Minimum soak seconds.
    #[serde(default)]
    pub min_seconds: Option<u64>,
    /// qtest case names for a `qtest` reader.
    #[serde(default)]
    pub cases: Vec<String>,
    /// Expansion kind (`oracle`, `crates`, `qtest`).
    #[serde(default)]
    pub expand: Option<String>,
    /// Dependency ticket id.
    #[serde(default)]
    pub ticket: Option<String>,
    /// Human note for the table.
    #[serde(default)]
    pub note: Option<String>,
}

/// Exit-code pass rule.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub struct PassRule {
    /// Required exit code.
    #[serde(default = "default_exit")]
    pub exit: i32,
}

/// Conditions that make a check `BLOCKED`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct BlockedOn {
    /// Exit codes that mean "prerequisite absent".
    #[serde(default)]
    pub exit: Vec<i32>,
    /// Missing files that mean "prerequisite absent".
    #[serde(default)]
    pub missing_files: Vec<String>,
    /// Ticket that must be `done` in `tickets/INDEX.md`.
    #[serde(default)]
    pub ticket: Option<String>,
}

/// The whole `rows.toml`.
#[derive(Clone, Debug, Deserialize)]
pub struct Rows {
    /// Checks in table order.
    #[serde(default)]
    pub check: Vec<CheckSpec>,
    /// Thresholds mirrored from the specification.
    #[serde(default)]
    pub thresholds: Thresholds,
}

/// Thresholds copied from `16-size-testing-oracles.md` 3.B.9 G1.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Thresholds {
    /// NetBSD donor suite percentage.
    #[serde(default = "pct_90")]
    pub netbsd: u64,
    /// FreeBSD donor suite percentage.
    #[serde(default = "pct_90")]
    pub freebsd: u64,
    /// OpenBSD donor suite percentage.
    #[serde(default = "pct_90")]
    pub openbsd: u64,
    /// picolibc tiny donor suite percentage.
    #[serde(default = "pct_90")]
    pub picolibc: u64,
    /// mlibc donor suite percentage.
    #[serde(default = "pct_85")]
    pub mlibc: u64,
    /// Maximum donor exclusions.
    #[serde(default = "default_250")]
    pub exclusions: u64,
    /// Fuzz soak seconds.
    #[serde(default = "default_3600")]
    pub soak_seconds: u64,
    /// World minimum built packages.
    #[serde(default = "default_100")]
    pub world_built: u64,
    /// World weighted rows.
    #[serde(default = "default_weighted")]
    pub world_weighted: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            netbsd: 90,
            freebsd: 90,
            openbsd: 90,
            picolibc: 90,
            mlibc: 85,
            exclusions: 250,
            soak_seconds: 3600,
            world_built: 100,
            world_weighted: 197.5,
        }
    }
}

const fn default_required() -> bool {
    true
}
const fn default_kind() -> String {
    String::new()
}
const fn default_exit() -> i32 {
    0
}
const fn pct_90() -> u64 {
    90
}
const fn pct_85() -> u64 {
    85
}
const fn default_250() -> u64 {
    250
}
const fn default_3600() -> u64 {
    3600
}
const fn default_100() -> u64 {
    100
}
const fn default_weighted() -> f64 {
    197.5
}

impl Rows {
    /// Read and validate a `rows.toml`.
    ///
    /// # Errors
    ///
    /// Returns an error for an unreadable file, TOML failure or invalid row definition.
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
        let rows: Self =
            toml::from_str(&text).with_context(|| format!("cannot parse {}", path.display()))?;
        rows.validate()?;
        Ok(rows)
    }

    /// Validate the invariants the evaluator relies on.
    ///
    /// # Errors
    ///
    /// Returns an error for duplicate ids, an unknown kind, a shell command, an empty evidence
    /// path or a row outside 1..=6.
    pub fn validate(&self) -> Result<()> {
        if self.check.is_empty() {
            bail!("rows.toml has no checks");
        }
        let mut ids = BTreeSet::new();
        for check in &self.check {
            if !ids.insert(check.id.as_str()) {
                bail!("duplicate check id `{}`", check.id);
            }
            if !(1..=6).contains(&check.row) {
                bail!("check `{}` has row {}", check.id, check.row);
            }
            if !matches!(check.kind.as_str(), "argv" | "read" | "info") {
                bail!("check `{}` has unknown kind `{}`", check.id, check.kind);
            }
            if check.evidence.is_empty() {
                bail!("check `{}` has no evidence path", check.id);
            }
            if !check.evidence.starts_with("build/libc-gate/") {
                bail!("check `{}` evidence is not under build/libc-gate", check.id);
            }
            match check.kind.as_str() {
                "argv" => {
                    let Some(first) = check.argv.first() else {
                        bail!("check `{}` has no argv", check.id);
                    };
                    if first != "cargo" {
                        bail!("check `{}` argv does not start with cargo", check.id);
                    }
                    if check.argv.iter().any(|arg| arg == "sh" || arg == "-c") {
                        bail!("check `{}` contains a shell", check.id);
                    }
                }
                "read" => {
                    if check.read.is_none() {
                        bail!("read check `{}` has no reader", check.id);
                    }
                    if check.file.is_none()
                        && !matches!(check.read.as_deref(), Some("qtest" | "mandoc"))
                    {
                        bail!("read check `{}` has no file", check.id);
                    }
                }
                "info" => {}
                _ => {}
            }
        }
        Ok(())
    }

    /// Look up a check by id.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&CheckSpec> {
        self.check.iter().find(|check| check.id == id)
    }
}

#[cfg(test)]
#[path = "rows_ut.rs"]
mod tests;

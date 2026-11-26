// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Fail-closed required-subset gate (S750).
//!
//! The aggregate G1 criteria in `gate.rs` are progress metrics: a high percentage can still
//! hide an unrun, skipped or failed kernel-facing body. This module evaluates a finite matrix
//! of individually required donor bodies. A required row is met only when a matching body
//! exists in the result table, its verdict is `pass`, and every declared prerequisite body
//! also passed. A missing matrix, an empty matrix or a missing row is a gate failure.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::table::ConformanceTable;
use crate::{Verdict, glob_match};

/// The required matrix.
#[derive(Clone, Debug, Deserialize)]
pub struct RequiredConfig {
    /// Schema version; 1.
    pub schema: u32,
    /// Required donor bodies.
    #[serde(default, rename = "required")]
    pub rows: Vec<RequiredRow>,
}

/// One required donor body.
#[derive(Clone, Debug, Deserialize)]
pub struct RequiredRow {
    /// Stable row id, e.g. `descriptors-dup-close`.
    pub id: String,
    /// Suite name (`freebsd-libc-sys`).
    pub suite: String,
    /// Program path, glob `*`/`?` allowed.
    pub program: String,
    /// Test case ident, glob `*`/`?` allowed; defaults to every body of the program.
    #[serde(default = "all_cases")]
    pub case: String,
    /// Exact donor file the body comes from.
    pub donor: String,
    /// Expected donor behaviour the body asserts.
    pub expect: String,
    /// Execution environment this row is required in.
    pub environment: String,
    /// Raven regression that complements the donor body.
    pub regression: String,
    /// Bodies that must pass before this row can count.
    #[serde(default)]
    pub prerequisite: Vec<Prerequisite>,
}

/// One prerequisite donor body.
#[derive(Clone, Debug, Deserialize)]
pub struct Prerequisite {
    /// Suite name.
    pub suite: String,
    /// Program path, glob allowed.
    pub program: String,
    /// Test case ident, glob allowed.
    #[serde(default = "all_cases")]
    pub case: String,
}

fn all_cases() -> String {
    "*".to_string()
}

impl RequiredConfig {
    /// Load a committed required matrix.
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
        let config: Self =
            toml::from_str(&text).with_context(|| format!("cannot parse {}", path.display()))?;
        if config.schema != 1 {
            bail!(
                "{} has schema {}, expected 1",
                path.display(),
                config.schema
            );
        }
        let mut ids = BTreeSet::new();
        for row in &config.rows {
            if row.id.is_empty() {
                bail!("{} has an empty required row id", path.display());
            }
            if !ids.insert(row.id.as_str()) {
                bail!("{} repeats required row `{}`", path.display(), row.id);
            }
            if row.suite.is_empty() || row.program.is_empty() {
                bail!(
                    "{} row `{}` needs a suite and a program",
                    path.display(),
                    row.id
                );
            }
            if row.donor.is_empty()
                || row.expect.is_empty()
                || row.environment.is_empty()
                || row.regression.is_empty()
            {
                bail!(
                    "{} row `{}` needs donor, expect, environment and regression",
                    path.display(),
                    row.id
                );
            }
        }
        Ok(config)
    }
}

/// One evaluated required row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequiredOutcome {
    /// Row id.
    pub id: String,
    /// Whether the row is met.
    pub met: bool,
    /// Why the row is not met; empty when met.
    pub note: String,
}

/// Evaluate the required matrix. Every row is independent.
#[must_use]
pub fn evaluate(config: &RequiredConfig, table: &ConformanceTable) -> Vec<RequiredOutcome> {
    config
        .rows
        .iter()
        .map(|row| {
            let matched = matching(table, &row.suite, &row.program, &row.case);
            if matched.is_empty() {
                return RequiredOutcome {
                    id: row.id.clone(),
                    met: false,
                    note: "missing: no result row".to_string(),
                };
            }
            if let Some(failed) = matched
                .iter()
                .find(|outcome| outcome.verdict != Verdict::Pass)
            {
                return RequiredOutcome {
                    id: row.id.clone(),
                    met: false,
                    note: format!(
                        "not pass: {} {}/{}/{}: {}",
                        failed.verdict,
                        failed.case.suite,
                        failed.case.program,
                        failed.case.name,
                        failed.reason
                    ),
                };
            }
            for prerequisite in &row.prerequisite {
                let matching = matching(
                    table,
                    &prerequisite.suite,
                    &prerequisite.program,
                    &prerequisite.case,
                );
                if matching.is_empty() {
                    return RequiredOutcome {
                        id: row.id.clone(),
                        met: false,
                        note: format!(
                            "prerequisite missing: {}/{}/{}",
                            prerequisite.suite, prerequisite.program, prerequisite.case
                        ),
                    };
                }
                if let Some(failed) = matching
                    .iter()
                    .find(|outcome| outcome.verdict != Verdict::Pass)
                {
                    return RequiredOutcome {
                        id: row.id.clone(),
                        met: false,
                        note: format!(
                            "prerequisite not pass: {}/{}/{}: {}",
                            prerequisite.suite,
                            prerequisite.program,
                            prerequisite.case,
                            failed.verdict
                        ),
                    };
                }
            }
            RequiredOutcome {
                id: row.id.clone(),
                met: true,
                note: String::new(),
            }
        })
        .collect()
}

fn matching<'a>(
    table: &'a ConformanceTable,
    suite: &str,
    program: &str,
    case: &str,
) -> Vec<&'a crate::Outcome> {
    table
        .rows
        .iter()
        .map(|row| &row.outcome)
        .filter(|outcome| {
            outcome.case.suite == suite
                && glob_match(program, &outcome.case.program)
                && glob_match(case, &outcome.case.name)
        })
        .collect()
}

/// Render the required rows and the final verdict.
#[must_use]
pub fn render(outcomes: &[RequiredOutcome]) -> String {
    let mut text = String::new();
    for outcome in outcomes {
        if outcome.met {
            text.push_str(&format!("REQUIRED {}: MET\n", outcome.id));
        } else {
            text.push_str(&format!(
                "REQUIRED {}: NOT MET ({})\n",
                outcome.id, outcome.note
            ));
        }
    }
    let met = outcomes.iter().filter(|outcome| outcome.met).count();
    text.push_str(&format!(
        "required: {} ({} of {} met)\n",
        if !outcomes.is_empty() && met == outcomes.len() {
            "PASS"
        } else {
            "FAIL"
        },
        met,
        outcomes.len()
    ));
    text
}

/// Whether every required row is met; an empty matrix is never a pass.
#[must_use]
pub fn met(outcomes: &[RequiredOutcome]) -> bool {
    !outcomes.is_empty() && outcomes.iter().all(|outcome| outcome.met)
}

#[cfg(test)]
#[path = "required_ut.rs"]
mod tests;

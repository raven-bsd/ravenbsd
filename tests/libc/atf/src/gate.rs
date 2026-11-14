// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! G1 conformance gate (ticket C71, 4.9; thresholds mirrored from 16 3.B.9).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::table::ConformanceTable;
use crate::{G1Row, Verdict};

/// The gate configuration.
#[derive(Clone, Debug, Deserialize)]
pub struct GateConfig {
    /// Schema version; 1.
    pub schema: u32,
    /// Criteria rows.
    #[serde(default, rename = "criterion")]
    pub criteria: Vec<Criterion>,
}

/// One G1 criterion.
#[derive(Clone, Debug, Deserialize)]
pub struct Criterion {
    /// `C2`..`C9`.
    pub id: String,
    /// Suites measured together.
    #[serde(default)]
    pub suites: Vec<String>,
    /// `bodies` or `programs`.
    #[serde(default = "bodies_unit")]
    pub unit: String,
    /// Threshold in percent.
    pub threshold: f64,
    /// Optional exclusion cap (C2).
    #[serde(default)]
    pub max_exclusions: Option<usize>,
    /// External tool, not evaluated here.
    #[serde(default)]
    pub external: bool,
}

fn bodies_unit() -> String {
    "bodies".to_string()
}

impl GateConfig {
    /// Load the committed `g1.toml`.
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
        let config: Self =
            toml::from_str(&text).with_context(|| format!("cannot parse {}", path.display()))?;
        if config.schema != 1 {
            bail!("g1.toml has schema {}, expected 1", config.schema);
        }
        for criterion in &config.criteria {
            if static_text(&criterion.id).is_none() {
                bail!("g1.toml has unknown criterion `{}`", criterion.id);
            }
        }
        Ok(config)
    }
}

/// Evaluate the gate over a table.
pub fn evaluate(config: &GateConfig, table: &ConformanceTable) -> Vec<G1Row> {
    let mut rows = Vec::new();
    for criterion in &config.criteria {
        let selected: Vec<&crate::table::TableRow> = table
            .rows
            .iter()
            .filter(|row| criterion.suites.contains(&row.outcome.case.suite))
            .collect();
        let id = static_id(&criterion.id);
        let text = static_text(&criterion.id).unwrap_or("unknown");
        if criterion.external {
            rows.push(G1Row {
                id,
                text,
                measured: None,
                threshold: criterion.threshold,
                met: false,
                note: "0|eligible|0|-|external: not evaluated".to_string(),
            });
            continue;
        }
        let mut exclusions = 0usize;
        let mut eligible = 0usize;
        let mut passed = 0usize;
        let mut not_run = 0usize;
        if criterion.unit == "programs" {
            let mut programs: BTreeMap<(String, String), Vec<&crate::table::TableRow>> =
                BTreeMap::new();
            for row in &selected {
                programs
                    .entry((
                        row.outcome.case.suite.clone(),
                        row.outcome.case.program.clone(),
                    ))
                    .or_default()
                    .push(row);
            }
            for rows in programs.values() {
                if rows
                    .iter()
                    .all(|row| row.outcome.verdict == Verdict::Excluded)
                {
                    exclusions += 1;
                    continue;
                }
                if rows
                    .iter()
                    .all(|row| row.outcome.verdict == Verdict::OracleFail)
                {
                    continue;
                }
                eligible += 1;
                if rows
                    .iter()
                    .all(|row| matches!(row.outcome.verdict, Verdict::Pass | Verdict::Skip))
                    && rows.iter().any(|row| row.outcome.verdict == Verdict::Pass)
                {
                    passed += 1;
                }
                if rows
                    .iter()
                    .all(|row| row.outcome.verdict == Verdict::NotRun)
                {
                    not_run += 1;
                }
            }
        } else {
            for row in &selected {
                match row.outcome.verdict {
                    Verdict::Excluded => exclusions += 1,
                    Verdict::OracleFail => {}
                    Verdict::NotRun => {
                        eligible += 1;
                        not_run += 1;
                    }
                    Verdict::Pass => {
                        eligible += 1;
                        passed += 1;
                    }
                    Verdict::Fail => eligible += 1,
                    Verdict::Skip => {
                        if !(row.outcome.reason.starts_with("needs-root")
                            || row.outcome.reason.starts_with("needs-program")
                            || row.outcome.reason.starts_with("needs-file"))
                        {
                            eligible += 1;
                        }
                    }
                }
            }
        }
        let measured = if eligible == 0 && selected.is_empty() {
            None
        } else if eligible == 0 {
            Some(0.0)
        } else {
            Some(passed as f64 * 100.0 / eligible as f64)
        };
        let cap_ok = criterion.max_exclusions.is_none_or(|cap| exclusions <= cap);
        let threshold_ok =
            measured.is_some_and(|value| value + f64::EPSILON >= criterion.threshold);
        let met = threshold_ok && cap_ok;
        let unit = if criterion.unit == "programs" {
            "programs"
        } else {
            "eligible"
        };
        let mut note = if selected.is_empty() {
            "not vendored (no rows)".to_string()
        } else if passed == 0 && not_run > 0 {
            "not run".to_string()
        } else {
            String::new()
        };
        if !cap_ok {
            note = if note.is_empty() {
                "exclusion cap exceeded".to_string()
            } else {
                format!("{note}, exclusion cap exceeded")
            };
        }
        rows.push(G1Row {
            id,
            text,
            measured,
            threshold: criterion.threshold,
            met,
            note: format!(
                "{eligible}|{unit}|{exclusions}|{}|{note}",
                criterion
                    .max_exclusions
                    .map_or_else(|| "-".to_string(), |cap| cap.to_string())
            ),
        });
    }
    rows
}

/// Render the gate lines exactly as 4.9 describes.
#[must_use]
pub fn render(rows: &[G1Row]) -> String {
    let mut text = String::new();
    let mut measured = 0usize;
    let mut met = 0usize;
    for row in rows {
        let (denominator, unit, exclusions, cap, note) = split_note(&row.note);
        if row.measured.is_none() && note.contains("external") {
            text.push_str(&format!(
                "{} {}: external: not evaluated\n",
                row.id, row.text
            ));
            continue;
        }
        let percentage = row.measured.unwrap_or(0.0);
        text.push_str(&format!(
            "{} {}: {:.1} % of {denominator} {unit} (threshold {:.1} %)",
            row.id, row.text, percentage, row.threshold
        ));
        if cap != "-" {
            text.push_str(&format!(", exclusions {exclusions}/{cap}"));
        }
        if !row.met {
            text.push_str(" -> NOT MET");
            if !note.is_empty() {
                text.push_str(&format!(" ({note})"));
            }
        } else {
            text.push_str(" -> MET");
        }
        text.push('\n');
        if row.measured.is_some() {
            measured += 1;
            if row.met {
                met += 1;
            }
        }
    }
    text.push_str(&format!(
        "G1: {} ({} of {} measured rows)\n",
        if met == measured && measured > 0 {
            "MET"
        } else {
            "NOT MET"
        },
        met,
        measured
    ));
    text
}

fn split_note(note: &str) -> (&str, &str, &str, &str, &str) {
    let mut parts = note.splitn(5, '|');
    (
        parts.next().unwrap_or("0"),
        parts.next().unwrap_or("eligible"),
        parts.next().unwrap_or("0"),
        parts.next().unwrap_or("-"),
        parts.next().unwrap_or(""),
    )
}

/// Whether the gate exits 0.
#[must_use]
pub fn met(rows: &[G1Row]) -> bool {
    let considered: Vec<&G1Row> = rows
        .iter()
        .filter(|row| !row.note.contains("external"))
        .collect();
    !considered.is_empty()
        && considered
            .iter()
            .all(|row| row.measured.is_some() && row.met)
}

const TEXTS: &[(&str, &str)] = &[
    ("C2", "NetBSD ATF libc+libm+libpthread"),
    ("C3", "FreeBSD libc+msun"),
    ("C4", "OpenBSD regress"),
    ("C5", "picolibc tiny"),
    ("C6", "mlibc ansi+posix+rtld"),
    ("C7", "LTP"),
    ("C8", "pjdfstest"),
    ("C9", "differential tests"),
];

fn static_id(id: &str) -> &'static str {
    TEXTS
        .iter()
        .find(|(candidate, _)| *candidate == id)
        .map_or("C?", |(candidate, _)| candidate)
}

fn static_text(id: &str) -> Option<&'static str> {
    TEXTS
        .iter()
        .find(|(candidate, _)| *candidate == id)
        .map(|(_, text)| *text)
}

#[cfg(test)]
#[path = "gate_ut.rs"]
mod tests;

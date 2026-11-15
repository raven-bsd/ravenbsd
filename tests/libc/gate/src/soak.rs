// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `SOAK.md` reader for the hardening row.

use crate::model::Status;

/// One row of the soak table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoakRow {
    /// Fuzz target.
    pub target: String,
    /// Soak seconds, `None` when the row reports a crash instead of a duration.
    pub seconds: Option<u64>,
    /// Executions, `None` when not reported.
    pub executions: Option<u64>,
    /// Crash count.
    pub crashes: u64,
    /// The target is listed as DEFERRED in `known-gaps.md`.
    pub deferred: bool,
}

/// The evaluated soak.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoakReport {
    /// Observed status.
    pub status: Status,
    /// Human reason.
    pub reason: String,
    /// Parsed rows, sorted by target.
    pub rows: Vec<SoakRow>,
}

/// Parse the markdown table in `SOAK.md`.
///
/// A row whose seconds column is `—` (a crash before the hour) keeps `seconds = None`.
#[must_use]
pub fn parse(text: &str) -> Vec<SoakRow> {
    let mut rows = Vec::new();
    let mut in_table = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("| target |") {
            in_table = true;
            continue;
        }
        if !in_table || !line.starts_with('|') {
            continue;
        }
        if line.starts_with("|---") {
            continue;
        }
        let columns: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        if columns.len() < 8 {
            continue;
        }
        let target = columns[0].to_string();
        if target.is_empty() || target == "target" {
            continue;
        }
        let seconds = columns[1]
            .split_whitespace()
            .next()
            .and_then(|value| value.replace(',', "").parse::<u64>().ok());
        let executions = columns[2].replace(',', "").parse::<u64>().ok();
        let crashes = columns[6].parse::<u64>().unwrap_or(0);
        rows.push(SoakRow {
            target,
            seconds,
            executions,
            crashes,
            deferred: false,
        });
    }
    rows.sort_by(|left, right| left.target.cmp(&right.target));
    rows
}

/// Extract the DEFERRED target names from `known-gaps.md`.
#[must_use]
pub fn deferred_targets(text: &str) -> Vec<String> {
    let mut in_deferred = false;
    let mut names = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("## ") {
            in_deferred = line.eq_ignore_ascii_case("## DEFERRED targets");
            continue;
        }
        if !in_deferred || !line.starts_with('|') || line.contains("target |") {
            continue;
        }
        let Some(start) = line.find('`') else {
            continue;
        };
        let Some(end) = line[start + 1..].find('`') else {
            continue;
        };
        names.push(line[start + 1..start + 1 + end].to_string());
    }
    names.sort();
    names
}

/// Mark the DEFERRED rows.
pub fn apply_deferred(rows: &mut [SoakRow], deferred: &[String]) {
    for row in rows {
        row.deferred = deferred.iter().any(|name| name == &row.target);
    }
}

/// Evaluate the soak rules of `16-size-testing-oracles.md` 3.B.9 C10.
///
/// Every expected target must have a row or be DEFERRED. Every non-DEFERRED row must soak at
/// least `min_seconds`, have non-zero executions and zero crashes.
#[must_use]
pub fn evaluate(
    rows: &[SoakRow],
    expected: &[String],
    deferred: &[String],
    min_seconds: u64,
) -> SoakReport {
    let mut rows = rows.to_vec();
    apply_deferred(&mut rows, deferred);
    if rows.is_empty() {
        return SoakReport {
            status: Status::Blocked,
            reason: "SOAK.md has no table rows".to_string(),
            rows,
        };
    }
    let mut problems = Vec::new();
    for target in expected {
        if !rows.iter().any(|row| &row.target == target) && !deferred.contains(target) {
            problems.push(format!("missing target {target}"));
        }
    }
    for row in &rows {
        if row.deferred {
            continue;
        }
        match row.seconds {
            Some(seconds) if seconds >= min_seconds => {}
            Some(seconds) => {
                problems.push(format!("{} soaked {seconds}s < {min_seconds}s", row.target))
            }
            None => problems.push(format!("{} has no soak duration", row.target)),
        }
        if row.crashes != 0 {
            problems.push(format!("{} has {} crash(es)", row.target, row.crashes));
        }
        if row.executions.unwrap_or(0) == 0 {
            problems.push(format!("{} has no executions", row.target));
        }
    }
    if problems.is_empty() {
        SoakReport {
            status: Status::Met,
            reason: format!(
                "{} targets soaked >= {min_seconds}s with zero crashes",
                rows.iter().filter(|row| !row.deferred).count()
            ),
            rows,
        }
    } else {
        SoakReport {
            status: Status::NotImplemented,
            reason: problems.join("; "),
            rows,
        }
    }
}

#[cfg(test)]
#[path = "soak_ut.rs"]
mod tests;

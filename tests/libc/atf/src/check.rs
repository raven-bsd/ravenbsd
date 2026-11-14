// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The C20 regression rule: regressions, vanished rows and unlisted bodies fail the check.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;

use crate::table::RawRow;
use crate::{Case, Verdict};

/// What `raven-atf-run check` found.
#[derive(Clone, Debug, Default)]
pub struct CheckReport {
    /// Baseline passes that no longer pass.
    pub regressions: Vec<(Case, String)>,
    /// Baseline rows with no result and no `removed` marker.
    pub vanished: Vec<Case>,
    /// Result rows absent from the baseline.
    pub unlisted: Vec<Case>,
    /// Baseline non-passes that now pass.
    pub improvements: Vec<Case>,
}

impl CheckReport {
    /// Whether the check must exit 1.
    #[must_use]
    pub fn failed(&self) -> bool {
        !self.regressions.is_empty() || !self.vanished.is_empty() || !self.unlisted.is_empty()
    }
}

/// Compare a run with the baseline.
pub fn check(baseline: &[RawRow], run: &[RawRow]) -> Result<CheckReport> {
    let mut report = CheckReport::default();
    let mut baseline_map: BTreeMap<Case, String> = BTreeMap::new();
    for row in baseline {
        if row.verdict == "removed" {
            continue;
        }
        let case = Case::new(&row.suite, &row.program, &row.case);
        baseline_map.insert(case, row.verdict.clone());
    }
    let mut run_map: BTreeMap<Case, String> = BTreeMap::new();
    let mut removed: BTreeSet<Case> = BTreeSet::new();
    for row in run {
        let case = Case::new(&row.suite, &row.program, &row.case);
        if row.verdict == "removed" {
            removed.insert(case);
            continue;
        }
        run_map.insert(case, row.verdict.clone());
    }
    for (case, before) in &baseline_map {
        match run_map.get(case) {
            Some(after) => {
                if before == "pass" && after != "pass" {
                    report.regressions.push((case.clone(), after.clone()));
                } else if before != "pass" && after == "pass" {
                    report.improvements.push(case.clone());
                }
            }
            None => {
                if !removed.contains(case) {
                    report.vanished.push(case.clone());
                }
            }
        }
    }
    for case in run_map.keys() {
        if !baseline_map.contains_key(case) {
            report.unlisted.push(case.clone());
        }
    }
    report
        .regressions
        .sort_by(|left, right| left.0.cmp(&right.0));
    report.vanished.sort();
    report.unlisted.sort();
    report.improvements.sort();
    Ok(report)
}

/// Convert run rows to outcomes for a report; rejects unknown verdicts other than `removed`.
pub fn run_verdicts(rows: &[RawRow]) -> Result<BTreeMap<Case, Verdict>> {
    let mut map = BTreeMap::new();
    for row in rows {
        if row.verdict == "removed" {
            continue;
        }
        let verdict = Verdict::parse(&row.verdict).map_err(|error| anyhow::anyhow!("{error}"))?;
        map.insert(Case::new(&row.suite, &row.program, &row.case), verdict);
    }
    Ok(map)
}

#[cfg(test)]
#[path = "check_ut.rs"]
mod tests;

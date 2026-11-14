// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! TSV baseline, merge of oracle/target runs, and the generated `CONFORMANCE.md` (C71 4.7).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::{Case, Outcome, Verdict};

/// One raw row of a conformance TSV.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawRow {
    /// Suite.
    pub suite: String,
    /// Program.
    pub program: String,
    /// Case.
    pub case: String,
    /// Verdict token (`removed` is accepted by `check` as an acknowledgement marker).
    pub verdict: String,
    /// Reason.
    pub reason: String,
    /// Ticket.
    pub ticket: String,
}

impl RawRow {
    /// Convert to an outcome, rejecting an unknown verdict.
    pub fn outcome(&self) -> Result<Outcome> {
        let verdict = Verdict::parse(&self.verdict).map_err(|error| anyhow::anyhow!("{error}"))?;
        Ok(Outcome::new(
            Case::new(&self.suite, &self.program, &self.case),
            verdict,
            self.reason.clone(),
            self.ticket.clone(),
        ))
    }
}

/// One merged row with the oracle-side fact needed by the gate.
#[derive(Clone, Debug)]
pub struct TableRow {
    /// The verdict after merging.
    pub outcome: Outcome,
    /// Whether the host oracle passed this body.
    pub oracle_pass: bool,
}

/// Totals for one suite.
#[derive(Clone, Debug, Default)]
pub struct SuiteStats {
    /// Suite name.
    pub suite: String,
    /// Bodies.
    pub bodies: usize,
    /// Programs.
    pub programs: usize,
    /// Programs that built against the target.
    pub target_builds: usize,
    /// Bodies that passed on the oracle.
    pub oracle_pass: usize,
    /// Bodies that passed on the target.
    pub target_pass: usize,
    /// Failures.
    pub fail: usize,
    /// Skips.
    pub skip: usize,
    /// Exclusions.
    pub excluded: usize,
    /// Oracle failures.
    pub oracle_fail: usize,
    /// Not run.
    pub not_run: usize,
    /// Bodies in the denominator.
    pub eligible: usize,
    /// Pass rate in percent.
    pub rate: f64,
}

impl SuiteStats {
    /// Fill counts from merged rows and target build statuses.
    #[must_use]
    pub fn from_rows(suite: &str, rows: &[TableRow], builds: &BTreeMap<String, bool>) -> Self {
        let mut stats = Self {
            suite: suite.to_string(),
            bodies: rows.len(),
            ..Self::default()
        };
        let mut programs = BTreeSet::new();
        for row in rows {
            programs.insert(row.outcome.case.program.clone());
            if row.oracle_pass {
                stats.oracle_pass += 1;
            }
            match row.outcome.verdict {
                Verdict::Pass => stats.target_pass += 1,
                Verdict::Fail => stats.fail += 1,
                Verdict::Skip => stats.skip += 1,
                Verdict::Excluded => stats.excluded += 1,
                Verdict::OracleFail => stats.oracle_fail += 1,
                Verdict::NotRun => stats.not_run += 1,
            }
        }
        stats.programs = programs.len();
        stats.target_builds = builds.values().filter(|built| **built).count();
        stats.eligible = stats
            .bodies
            .saturating_sub(stats.excluded)
            .saturating_sub(stats.oracle_fail)
            .saturating_sub(
                rows.iter()
                    .filter(|row| requirement_skip(&row.outcome))
                    .count(),
            );
        stats.rate = if stats.eligible == 0 {
            0.0
        } else {
            (stats.target_pass as f64) * 100.0 / (stats.eligible as f64)
        };
        stats
    }
}

fn requirement_skip(outcome: &Outcome) -> bool {
    outcome.verdict == Verdict::Skip
        && (outcome.reason.starts_with("needs-root")
            || outcome.reason.starts_with("needs-program")
            || outcome.reason.starts_with("needs-file"))
}

/// A merged conformance table.
#[derive(Clone, Debug)]
pub struct ConformanceTable {
    /// Every row, sorted.
    pub rows: Vec<TableRow>,
    /// Per-suite statistics.
    pub stats: Vec<SuiteStats>,
}

/// Read a TSV without interpreting verdicts.
pub fn read_raw(path: &Path) -> Result<Vec<RawRow>> {
    let text =
        fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    let mut rows = Vec::new();
    for (number, line) in text.lines().enumerate() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 4 || fields.len() > 6 {
            bail!(
                "{}:{}: expected 4-6 tab-separated fields, got {}",
                path.display(),
                number + 1,
                fields.len()
            );
        }
        let get = |index: usize| fields.get(index).copied().unwrap_or("").to_string();
        rows.push(RawRow {
            suite: get(0),
            program: get(1),
            case: get(2),
            verdict: get(3),
            reason: get(4),
            ticket: get(5),
        });
    }
    rows.sort_by(|left, right| {
        (&left.suite, &left.program, &left.case).cmp(&(&right.suite, &right.program, &right.case))
    });
    Ok(rows)
}

/// Write outcomes as a TSV with the SPDX header and `# schema 1`.
pub fn write_outcomes(path: &Path, outcomes: &[Outcome]) -> Result<()> {
    let mut sorted: Vec<&Outcome> = outcomes.iter().collect();
    sorted.sort_by(|left, right| left.case.cmp(&right.case));
    let mut text = String::from(
        "# SPDX-License-Identifier: BSD-3-Clause\n# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors\n# schema 1\n",
    );
    for outcome in sorted {
        text.push_str(&format_outcome(outcome));
        text.push('\n');
    }
    write_text(path, &text)
}

fn format_outcome(outcome: &Outcome) -> String {
    let mut fields = vec![
        outcome.case.suite.clone(),
        outcome.case.program.clone(),
        outcome.case.name.clone(),
        outcome.verdict.as_str().to_string(),
        outcome.reason.clone(),
        outcome.ticket.clone(),
    ];
    while fields.len() > 4 && fields.last().is_some_and(String::is_empty) {
        fields.pop();
    }
    fields.join("\t")
}

/// Write per-program build statuses.
pub fn write_builds(path: &Path, builds: &BTreeMap<String, crate::BuildStatus>) -> Result<()> {
    let mut text = String::from("# SPDX-License-Identifier: BSD-3-Clause\n# schema 1\n");
    for (program, status) in builds {
        text.push_str(&format!(
            "{program}\t{}\t{}\n",
            status.token(),
            status.reason().replace('\n', " ")
        ));
    }
    write_text(path, &text)
}

/// Read per-program build statuses; `true` means built.
pub fn read_builds(path: &Path) -> Result<BTreeMap<String, bool>> {
    let mut builds = BTreeMap::new();
    if !path.is_file() {
        return Ok(builds);
    }
    let text =
        fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() >= 2 {
            builds.insert(fields[0].to_string(), fields[1] == "built");
        }
    }
    Ok(builds)
}

/// Merge `<runs>/oracle/*.tsv` and `<runs>/target/*.tsv` into the 4.7 table.
pub fn build_from_runs(runs: &Path) -> Result<ConformanceTable> {
    let mut suites = BTreeSet::new();
    for directory in ["oracle", "target"] {
        let path = runs.join(directory);
        if let Ok(entries) = fs::read_dir(&path) {
            for entry in entries {
                let entry = entry.context("cannot read a run entry")?;
                let name = entry.file_name().to_string_lossy().into_owned();
                if let Some(suite) = name.strip_suffix(".tsv")
                    && !suite.ends_with(".builds")
                {
                    suites.insert(suite.to_string());
                }
            }
        }
    }
    let mut rows = Vec::new();
    let mut stats = Vec::new();
    for suite in suites {
        let oracle_path = runs.join("oracle").join(format!("{suite}.tsv"));
        let target_path = runs.join("target").join(format!("{suite}.tsv"));
        let oracle = if oracle_path.is_file() {
            read_raw(&oracle_path)?
                .into_iter()
                .map(|row| row.outcome())
                .collect::<Result<Vec<_>>>()?
        } else {
            Vec::new()
        };
        let target = if target_path.is_file() {
            read_raw(&target_path)?
                .into_iter()
                .map(|row| row.outcome())
                .collect::<Result<Vec<_>>>()?
        } else {
            Vec::new()
        };
        let oracle_map: BTreeMap<Case, Outcome> = oracle
            .into_iter()
            .map(|outcome| (outcome.case.clone(), outcome))
            .collect();
        let target_map: BTreeMap<Case, Outcome> = target
            .into_iter()
            .map(|outcome| (outcome.case.clone(), outcome))
            .collect();
        let mut cases: BTreeSet<Case> = oracle_map.keys().cloned().collect();
        cases.extend(target_map.keys().cloned());
        let mut suite_rows = Vec::new();
        for case in cases {
            let oracle_row = oracle_map.get(&case);
            let target_row = target_map.get(&case);
            let oracle_pass = oracle_row.is_some_and(|row| row.verdict == Verdict::Pass);
            let (verdict, reason, ticket) = merge_row(oracle_row, target_row);
            suite_rows.push(TableRow {
                outcome: Outcome::new(case, verdict, reason, ticket),
                oracle_pass,
            });
        }
        suite_rows.sort_by(|left, right| left.outcome.case.cmp(&right.outcome.case));
        let builds = read_builds(&runs.join("target").join(format!("{suite}.builds.tsv")))?;
        stats.push(SuiteStats::from_rows(&suite, &suite_rows, &builds));
        rows.extend(suite_rows);
    }
    Ok(ConformanceTable { rows, stats })
}

fn merge_row(oracle: Option<&Outcome>, target: Option<&Outcome>) -> (Verdict, String, String) {
    if let Some(oracle) = oracle {
        match oracle.verdict {
            Verdict::OracleFail => {
                return (Verdict::OracleFail, oracle.reason.clone(), "-".to_string());
            }
            Verdict::Excluded => {
                return (Verdict::Excluded, oracle.reason.clone(), "-".to_string());
            }
            Verdict::Skip => {
                return (Verdict::Skip, oracle.reason.clone(), oracle.ticket.clone());
            }
            Verdict::Pass => {}
            Verdict::Fail => {
                return (
                    Verdict::OracleFail,
                    format!("oracle-behaviour:{}", oracle.reason),
                    "-".to_string(),
                );
            }
            Verdict::NotRun => {
                return (
                    Verdict::NotRun,
                    oracle.reason.clone(),
                    oracle.ticket.clone(),
                );
            }
        }
    }
    match target {
        Some(target) => (target.verdict, target.reason.clone(), target.ticket.clone()),
        None => (
            Verdict::NotRun,
            "no-target-build".to_string(),
            oracle.map_or_else(|| "-".to_string(), |row| row.ticket.clone()),
        ),
    }
}

/// Write the authoritative baseline TSV.
pub fn write_baseline(path: &Path, table: &ConformanceTable) -> Result<()> {
    for row in &table.rows {
        if matches!(
            row.outcome.verdict,
            Verdict::Fail | Verdict::Skip | Verdict::NotRun
        ) && crate::owners::fail_without_owner(&row.outcome.ticket)
        {
            bail!(
                "table: {} row {}/{}/{} has no owning ticket",
                row.outcome.verdict.as_str(),
                row.outcome.case.suite,
                row.outcome.case.program,
                row.outcome.case.name
            );
        }
    }
    let outcomes: Vec<Outcome> = table.rows.iter().map(|row| row.outcome.clone()).collect();
    write_outcomes(path, &outcomes)
}

/// Regressions against a baseline: `(case, baseline, now)`.
pub fn regressions(
    baseline: &[RawRow],
    table: &ConformanceTable,
) -> Result<Vec<(Case, String, String)>> {
    let now: BTreeMap<Case, Verdict> = table
        .rows
        .iter()
        .map(|row| (row.outcome.case.clone(), row.outcome.verdict))
        .collect();
    let mut regressed = Vec::new();
    for row in baseline {
        if row.verdict == "removed" {
            continue;
        }
        if row.verdict != "pass" {
            continue;
        }
        let case = Case::new(&row.suite, &row.program, &row.case);
        if let Some(verdict) = now.get(&case)
            && *verdict != Verdict::Pass
        {
            regressed.push((case, row.verdict.clone(), verdict.as_str().to_string()));
        }
    }
    Ok(regressed)
}

/// Render `CONFORMANCE.md`.
#[must_use]
pub fn render_markdown(table: &ConformanceTable) -> String {
    let mut text = String::from(
        "<!-- SPDX-License-Identifier: BSD-3-Clause -->\n<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->\n\n# ravenlibc donor-suite conformance\n\nNothing is deleted from this table: a failing body stays listed until it passes (rule of C20).\n\n",
    );
    text.push_str("| Suite | bodies | programs | target-builds | oracle-pass | target-pass | fail | skip | excluded | oracle-fail | not-run | rate |\n");
    text.push_str("|---|---|---|---|---|---|---|---|---|---|---|---|\n");
    for stats in &table.stats {
        text.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {:.1} % |\n",
            stats.suite,
            stats.bodies,
            stats.programs,
            stats.target_builds,
            stats.oracle_pass,
            stats.target_pass,
            stats.fail,
            stats.skip,
            stats.excluded,
            stats.oracle_fail,
            stats.not_run,
            stats.rate
        ));
    }
    for stats in &table.stats {
        let suite_rows: Vec<&TableRow> = table
            .rows
            .iter()
            .filter(|row| row.outcome.case.suite == stats.suite)
            .collect();
        let non_pass: Vec<&TableRow> = suite_rows
            .iter()
            .copied()
            .filter(|row| row.outcome.verdict != Verdict::Pass)
            .collect();
        if !non_pass.is_empty() {
            text.push_str(&format!(
                "\n#### {}: rows that are not `pass`\n| program | case | verdict | reason | ticket |\n|---|---|---|---|---|\n",
                stats.suite
            ));
            for row in non_pass {
                text.push_str(&format!(
                    "| {} | {} | {} | {} | {} |\n",
                    escape_md(&row.outcome.case.program),
                    escape_md(&row.outcome.case.name),
                    row.outcome.verdict.as_str(),
                    escape_md(&row.outcome.reason),
                    escape_md(&row.outcome.ticket)
                ));
            }
        }
        text.push_str(&format!("\nPrograms of {}:\n\n", stats.suite));
        let mut programs: BTreeMap<String, usize> = BTreeMap::new();
        for row in &suite_rows {
            let entry = programs
                .entry(row.outcome.case.program.clone())
                .or_default();
            if row.outcome.verdict == Verdict::Pass {
                *entry += 1;
            }
        }
        for (program, count) in programs {
            text.push_str(&format!("* {program}: {count} pass\n"));
        }
    }
    text
}

fn escape_md(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

/// Write markdown to a file.
pub fn write_markdown(path: &Path, table: &ConformanceTable) -> Result<()> {
    write_text(path, &render_markdown(table))
}

fn write_text(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    fs::write(path, text).with_context(|| format!("cannot write {}", path.display()))
}

/// The default runs directory.
#[must_use]
pub fn default_runs(root: &Path) -> PathBuf {
    root.join("build/libc-conformance")
}

#[cfg(test)]
#[path = "table_ut.rs"]
mod tests;

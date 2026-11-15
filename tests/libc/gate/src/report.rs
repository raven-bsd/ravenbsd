// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Stored-result loading, table rendering and the verdict.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::model::{CheckResult, ROW_TITLES, Status, aggregate, verdict as gate_verdict};
use crate::rows::Rows;

/// Load every `*.json` result in `out`.
///
/// # Errors
///
/// Returns an error for an unreadable directory or malformed result file.
pub fn load_results(out: &Path) -> Result<Vec<CheckResult>> {
    let mut results = Vec::new();
    if !out.is_dir() {
        return Ok(results);
    }
    for entry in fs::read_dir(out).with_context(|| format!("cannot list {}", out.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let text =
            fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
        let result: CheckResult = serde_json::from_str(&text)
            .with_context(|| format!("cannot parse {}", path.display()))?;
        results.push(result);
    }
    results.sort_by_key(|left| natural_key(left.row, &left.id));
    Ok(results)
}

/// A deterministic sort key for ids such as `1.4`, `5.1.libc-hello`.
fn natural_key(row: u8, id: &str) -> (u8, Vec<(u8, String)>) {
    let mut parts = Vec::new();
    for part in id.split('.') {
        let numeric = part.chars().all(|character| character.is_ascii_digit());
        let (kind, text) = if numeric {
            (0, part.to_string())
        } else {
            (1, part.to_string())
        };
        parts.push((kind, text));
    }
    (row, parts)
}

/// Fill in a missing required check as NOT MET.
fn missing(id: &str, row: u8, title: &str) -> CheckResult {
    CheckResult {
        id: id.to_string(),
        row,
        title: title.to_string(),
        status: Status::NotImplemented,
        exit: None,
        reason: "no result; evidence file missing".to_string(),
        evidence: format!("build/libc-gate/{id}.txt"),
    }
}

/// Resolve one spec to a result, aggregating expanded children when needed.
fn resolve(spec: &crate::rows::CheckSpec, results: &[CheckResult]) -> CheckResult {
    if let Some(result) = results.iter().find(|result| result.id == spec.id) {
        return result.clone();
    }
    let prefix = format!("{}.", spec.id);
    let children: Vec<&CheckResult> = results
        .iter()
        .filter(|result| result.id.starts_with(&prefix))
        .collect();
    if children.is_empty() {
        return missing(&spec.id, spec.row, &spec.title);
    }
    let status = aggregate(children.iter().map(|child| child.status));
    CheckResult {
        id: spec.id.clone(),
        row: spec.row,
        title: spec.title.clone(),
        status,
        exit: None,
        reason: format!("{} expanded checks", children.len()),
        evidence: format!("build/libc-gate/{}.txt", spec.id),
    }
}

/// Render the gate table from stored results.
#[must_use]
pub fn render(results: &[CheckResult], rows: &Rows) -> String {
    let mut lines = vec![
        "| Row | Check | Status | Evidence | Note |".to_string(),
        "|---|---|---|---|---|".to_string(),
    ];
    for spec in &rows.check {
        let result = resolve(spec, results);
        let note = if result.reason.is_empty() {
            spec.note.clone().unwrap_or_default()
        } else {
            result.reason.clone()
        };
        lines.push(format!(
            "| {} | {} {} | {} | {} | {} |",
            ROW_TITLES[usize::from(result.row.saturating_sub(1))],
            result.id,
            result.title,
            result.status,
            result.evidence,
            note.replace('\n', " ")
        ));
    }
    lines.push(String::new());
    lines.push(format!("Verdict: {}", verdict_line(results, rows).0));
    lines.join("\n")
}

/// Compute the gate verdict from specs and stored results.
#[must_use]
pub fn verdict_line(results: &[CheckResult], rows: &Rows) -> (Status, [Status; 6]) {
    let mut row_status = [Status::Blocked; 6];
    for row in 1..=6u8 {
        let specs: Vec<&crate::rows::CheckSpec> =
            rows.check.iter().filter(|spec| spec.row == row).collect();
        let statuses: Vec<Status> = specs
            .iter()
            .map(|spec| {
                let result = resolve(spec, results);
                if spec.required {
                    result.status
                } else {
                    Status::Info
                }
            })
            .collect();
        let has_required = specs.iter().any(|spec| spec.required);
        row_status[usize::from(row - 1)] = if statuses.is_empty() {
            Status::Blocked
        } else if !has_required {
            Status::Info
        } else {
            aggregate(statuses)
        };
    }
    (gate_verdict(row_status), row_status)
}

/// Read the default result directory.
///
/// # Errors
///
/// Returns an error for an unreadable result directory.
pub fn load_default(out: &Path) -> Result<Vec<CheckResult>> {
    load_results(out)
}

#[cfg(test)]
#[path = "report_ut.rs"]
mod tests;

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Per-suite Markdown and JSON summaries.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::result::{Results, Status};

/// Counts for one suite.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SummaryRow {
    /// Suite name.
    pub suite: String,
    /// Number of results.
    pub total: usize,
    /// Passed results.
    pub pass: usize,
    /// Failed results.
    pub fail: usize,
    /// Skipped results.
    pub skip: usize,
    /// Broken results.
    pub broken: usize,
    /// `pass / total` as a percentage.
    pub pass_rate: f64,
}

/// Aggregate one or more result files by suite.
pub fn summarize(all: &[Results]) -> Vec<SummaryRow> {
    let mut rows: BTreeMap<String, SummaryRow> = BTreeMap::new();
    for results in all {
        for result in &results.results {
            let row = rows
                .entry(result.suite.clone())
                .or_insert_with(|| SummaryRow {
                    suite: result.suite.clone(),
                    total: 0,
                    pass: 0,
                    fail: 0,
                    skip: 0,
                    broken: 0,
                    pass_rate: 0.0,
                });
            row.total += 1;
            match result.status {
                Status::Pass => row.pass += 1,
                Status::Fail => row.fail += 1,
                Status::Skip => row.skip += 1,
                Status::Broken => row.broken += 1,
            }
        }
    }
    let mut rows: Vec<SummaryRow> = rows.into_values().collect();
    for row in &mut rows {
        if row.total > 0 {
            row.pass_rate = row.pass as f64 * 100.0 / row.total as f64;
        }
    }
    rows
}

/// Render the Markdown table.
pub fn markdown(rows: &[SummaryRow]) -> String {
    let mut out = String::from(
        "| Suite | Total | Pass | Fail | Skip | Broken | Pass rate |\n\
         |---|---:|---:|---:|---:|---:|---:|\n",
    );
    for row in rows {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {:.1}% |\n",
            row.suite, row.total, row.pass, row.fail, row.skip, row.broken, row.pass_rate
        ));
    }
    out
}

/// Render the rows as pretty JSON.
pub fn json(rows: &[SummaryRow]) -> serde_json::Result<String> {
    serde_json::to_string_pretty(rows)
}

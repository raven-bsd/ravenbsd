// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Static audit of the merged independent RACK model group.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// One audit row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// Model group.
    pub group: String,
    /// Donor function or value.
    pub donor: String,
    /// Model crate path.
    pub model_crate: String,
    /// Source file within the crate.
    pub source: String,
    /// Required source token.
    pub token: String,
    /// Named test.
    pub test: String,
    /// Mutation evidence string.
    pub mutation: String,
    /// Whether the donor is in the 219-function `rack.c` manifest.
    pub k315: bool,
}

/// Audit result.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Audited rows.
    pub rows: usize,
    /// Exact gaps.
    pub gaps: Vec<String>,
}

/// Load the TSV audit manifest.
///
/// # Errors
/// Returns a string when a line has the wrong field count or a boolean marker is invalid.
pub fn load(path: &Path) -> Result<Vec<Row>, String> {
    let text = fs::read_to_string(path).map_err(|error| format!("{path:?}: {error}"))?;
    let mut rows = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('|').map(str::trim).collect();
        if fields.len() != 8 {
            return Err(format!("audit row has {} fields: {line}", fields.len()));
        }
        rows.push(Row {
            group: fields[0].to_string(),
            donor: fields[1].to_string(),
            model_crate: fields[2].to_string(),
            source: fields[3].to_string(),
            token: fields[4].to_string(),
            test: fields[5].to_string(),
            mutation: fields[6].to_string(),
            k315: match fields[7] {
                "yes" => true,
                "no" => false,
                other => return Err(format!("invalid k315 marker {other:?}")),
            },
        });
    }
    Ok(rows)
}

fn read(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

fn rust_text(path: &Path) -> String {
    let mut text = String::new();
    let Ok(entries) = fs::read_dir(path) else {
        return text;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            text.push_str(&rust_text(&path));
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs")
            && let Some(content) = read(&path)
        {
            text.push_str(&content);
        }
    }
    text
}

/// Validate the rows against the repository.
#[must_use]
pub fn validate(root: &Path, rows: &[Row]) -> Report {
    let mut report = Report {
        rows: rows.len(),
        gaps: Vec::new(),
    };
    let mut seen = BTreeSet::new();
    let functions =
        read(&root.join("tests/rack-coverage/fixtures/rack-functions.txt")).unwrap_or_default();
    let mutations = read(&root.join("tests/rack-audit-models/mutations.tsv")).unwrap_or_default();

    for row in rows {
        if !seen.insert((row.group.clone(), row.donor.clone())) {
            report
                .gaps
                .push(format!("duplicate audit row {}/{}", row.group, row.donor));
        }
        let crate_path = root.join(&row.model_crate);
        if !crate_path.is_dir() {
            report
                .gaps
                .push(format!("missing model crate {}", row.model_crate));
            continue;
        }
        let source_path = crate_path.join(&row.source);
        let Some(source) = read(&source_path) else {
            report
                .gaps
                .push(format!("missing source {}", source_path.display()));
            continue;
        };
        if !source.contains(&row.token) {
            report.gaps.push(format!(
                "{}: token {:?} not in {}",
                row.donor, row.token, row.source
            ));
        }
        let tests = rust_text(&crate_path);
        if !tests.contains(&format!("fn {}", row.test)) {
            report
                .gaps
                .push(format!("{}: test {} not found", row.donor, row.test));
        }
        if !mutations.contains(&row.mutation) {
            report.gaps.push(format!(
                "{}: mutation {:?} not in mutations.tsv",
                row.donor, row.mutation
            ));
        }
        if row.k315 {
            let count = functions
                .lines()
                .filter(|line| line.split_whitespace().nth(1) == Some(row.donor.as_str()))
                .count();
            if count != 1 {
                report.gaps.push(format!(
                    "{}: K315 manifest count {count}, expected 1",
                    row.donor
                ));
            }
        }
    }
    report
}

/// Check pinned constants against source literals.
#[must_use]
pub fn validate_constants(root: &Path) -> Vec<String> {
    let path = root.join("tests/rack-audit-models/constants.tsv");
    let Some(text) = read(&path) else {
        return vec![format!("missing {}", path.display())];
    };
    let mut gaps = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('|').map(str::trim).collect();
        if fields.len() != 3 {
            gaps.push(format!("bad constants row: {line}"));
            continue;
        }
        let (file, name, expected) = (fields[0], fields[1], fields[2]);
        let Some(source) = read(&root.join(file)) else {
            gaps.push(format!("missing {file}"));
            continue;
        };
        let Some(value) = constant_value(&source, name) else {
            gaps.push(format!("{file}: constant {name} not found"));
            continue;
        };
        let Ok(expected) = parse_number(expected) else {
            gaps.push(format!("{file}: bad expected value {expected}"));
            continue;
        };
        if value != expected {
            gaps.push(format!("{file}: {name} is {value}, expected {expected}"));
        }
    }
    gaps
}

fn constant_value(source: &str, name: &str) -> Option<u64> {
    for line in source.lines() {
        let line = line.trim();
        if !(line.starts_with("pub const ") || line.starts_with("const ")) {
            continue;
        }
        let rest = line
            .strip_prefix("pub const ")
            .or_else(|| line.strip_prefix("const "))?;
        if !rest.starts_with(name) {
            continue;
        }
        let (_, value) = rest.split_once('=')?;
        let value = value.trim().trim_end_matches(';').trim();
        return parse_number(value).ok();
    }
    None
}

fn parse_number(value: &str) -> Result<u64, ()> {
    let value = value.replace('_', "");
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16).map_err(|_| ())
    } else {
        value.parse().map_err(|_| ())
    }
}

/// Count data rows in a trace file.
#[must_use]
pub fn trace_rows(path: &Path) -> usize {
    read(path)
        .map(|text| {
            text.lines()
                .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
                .count()
        })
        .unwrap_or(0)
}

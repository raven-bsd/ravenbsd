// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Group-report coverage rows used to fill the canonical register.
//!
//! The reports do not all use the same table shape: K328 has no status column, K335 has no Rust
//! column, and K334 has no coverage table. The audit accepts the shapes that exist and verifies
//! the mapping/test against the product source rather than trusting a missing cell.

use std::path::{Path, PathBuf};

use rack_coverage::{Row, parse_matrix};

/// One coverage row from a group report.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct GroupRow {
    /// Report ID (`K269`, `K328`, ...).
    pub group: String,
    /// Donor function name.
    pub function: String,
    /// Claimed Rust mapping.
    pub rust: String,
    /// Claimed named test(s).
    pub test: String,
    /// Claimed mutation/negative check.
    pub mutation: String,
}

/// Group reports that make up the RACK port union.
pub const GROUP_REPORTS: &[&str] = &[
    "K269", "K328", "K329", "K330", "K331", "K332", "K333", "K334", "K335", "K336",
];

/// Load all group-report coverage rows from a `tickets` directory.
#[must_use]
pub fn load_group_rows(root: &Path) -> Vec<GroupRow> {
    let mut rows = Vec::new();
    for id in GROUP_REPORTS {
        let Some(path) = find_report(root, id) else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let mut report_rows = parse_report(id, &text);
        rows.append(&mut report_rows);
    }
    rows
}

/// Find `Kxxx.md` either directly under `root` or under `<root>/<lane>/reports/`.
#[must_use]
pub fn find_report(root: &Path, id: &str) -> Option<PathBuf> {
    let direct = root.join(format!("{id}.md"));
    if direct.is_file() {
        return Some(direct);
    }
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path().join("reports").join(format!("{id}.md"));
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

/// Parse every coverage-shaped table in one report.
#[must_use]
pub fn parse_report(group: &str, text: &str) -> Vec<GroupRow> {
    let mut rows = Vec::new();
    for block in markdown_tables(text) {
        let parsed = parse_matrix(&block);
        if !parsed.is_empty() {
            for row in parsed {
                rows.push(from_row(group, row));
            }
            continue;
        }
        for row in parse_three_column(&block) {
            rows.push(GroupRow {
                group: group.to_owned(),
                ..row
            });
        }
    }
    rows.sort();
    rows.dedup();
    rows
}

fn from_row(group: &str, row: Row) -> GroupRow {
    GroupRow {
        group: group.to_owned(),
        function: row.function,
        rust: row.rust,
        test: row.test,
        mutation: row.mutation,
    }
}

/// Parse the K335 `Donor function | Named test | Mutation` shape.
fn parse_three_column(block: &str) -> Vec<GroupRow> {
    let lines: Vec<&str> = block.lines().collect();
    let Some(header) = lines.first() else {
        return Vec::new();
    };
    let columns: Vec<String> = cells(header)
        .iter()
        .map(|cell| cell.to_lowercase())
        .collect();
    let Some(function) = columns.iter().position(|cell| cell.contains("function")) else {
        return Vec::new();
    };
    let Some(test) = columns.iter().position(|cell| cell.contains("test")) else {
        return Vec::new();
    };
    let Some(mutation) = columns.iter().position(|cell| cell.contains("mutation")) else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for line in lines.iter().skip(1) {
        let values = cells(line);
        if values.is_empty() || values.iter().all(|cell| is_separator(cell)) {
            continue;
        }
        let Some(name) = backticked(values.get(function).copied().unwrap_or_default()) else {
            continue;
        };
        rows.push(GroupRow {
            group: String::new(),
            function: name,
            rust: String::new(),
            test: clean(values.get(test).copied().unwrap_or_default()),
            mutation: clean(values.get(mutation).copied().unwrap_or_default()),
        });
    }
    rows
}

fn markdown_tables(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut tables = Vec::new();
    let mut index = 0usize;
    while index < lines.len() {
        if !lines[index].trim_start().starts_with('|') {
            index += 1;
            continue;
        }
        let start = index;
        while index < lines.len() && lines[index].trim_start().starts_with('|') {
            index += 1;
        }
        tables.push(lines[start..index].join("\n"));
    }
    tables
}

fn cells(line: &str) -> Vec<&str> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .collect()
}

fn is_separator(cell: &str) -> bool {
    !cell.is_empty()
        && cell
            .chars()
            .all(|character| character == '-' || character == ':' || character.is_whitespace())
}

fn backticked(cell: &str) -> Option<String> {
    let start = cell.find('`')?;
    let rest = &cell[start + 1..];
    let end = rest.find('`')?;
    Some(rest[..end].trim().to_owned())
}

fn clean(cell: &str) -> String {
    cell.trim().trim_matches('`').trim().to_owned()
}

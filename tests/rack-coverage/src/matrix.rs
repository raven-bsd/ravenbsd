// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Markdown coverage-matrix parser and strict validator.

use crate::donor::Function;

/// One coverage-matrix row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// Donor function name.
    pub function: String,
    /// Rust function or module that implements the donor function.
    pub rust: String,
    /// Named Rust test(s) for the mapping.
    pub test: String,
    /// Mutation/negative check.
    pub mutation: String,
    /// Acceptance status; must be `PASS`.
    pub status: String,
}

/// One table's column positions.
#[derive(Clone, Copy, Debug)]
struct Columns {
    function: usize,
    rust: usize,
    test: usize,
    mutation: usize,
    status: Option<usize>,
}

/// Parse coverage rows from the first table whose header names function, Rust, test and mutation.
pub fn parse_matrix(markdown: &str) -> Vec<Row> {
    let lines: Vec<&str> = markdown.lines().collect();
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
        let block = &lines[start..index];
        let Some(columns) = header_columns(block.first().copied().unwrap_or_default()) else {
            continue;
        };
        let mut rows = Vec::new();
        for line in block.iter().skip(1) {
            let cells = cells(line);
            if cells.is_empty()
                || cells.iter().all(|cell| {
                    cell.chars().all(|character| {
                        character == '-' || character == ':' || character.is_whitespace()
                    })
                })
            {
                continue;
            }
            let value = |position: Option<usize>| {
                position
                    .and_then(|position| cells.get(position))
                    .map(|cell| clean(cell))
                    .unwrap_or_default()
            };
            let function = function_name(cells.get(columns.function).copied().unwrap_or_default());
            if function.is_empty() {
                continue;
            }
            rows.push(Row {
                function,
                rust: value(Some(columns.rust)),
                test: value(Some(columns.test)),
                mutation: value(Some(columns.mutation)),
                status: value(columns.status),
            });
        }
        return rows;
    }
    Vec::new()
}

fn header_columns(line: &str) -> Option<Columns> {
    let header = cells(line);
    let lower: Vec<String> = header
        .iter()
        .map(|cell| clean(cell).to_lowercase())
        .collect();
    let function = lower.iter().position(|cell| cell.contains("function"))?;
    let rust = lower.iter().position(|cell| cell.contains("rust"))?;
    let test = lower
        .iter()
        .position(|cell| cell.contains("test") || cell.contains("verification"))?;
    let mutation = lower.iter().position(|cell| cell.contains("mutation"))?;
    let status = lower.iter().position(|cell| cell.contains("status"));
    Some(Columns {
        function,
        rust,
        test,
        mutation,
        status,
    })
}

fn cells(line: &str) -> Vec<&str> {
    let trimmed = line.trim().trim_matches('|');
    trimmed.split('|').map(str::trim).collect()
}

fn clean(cell: &str) -> String {
    let trimmed = cell.trim();
    let trimmed = trimmed.strip_prefix('`').unwrap_or(trimmed);
    let trimmed = trimmed.strip_suffix('`').unwrap_or(trimmed);
    trimmed.trim().to_owned()
}

fn function_name(cell: &str) -> String {
    let cell = cell.trim();
    if let Some(start) = cell.find('`')
        && let Some(end) = cell[start + 1..].find('`')
    {
        return cell[start + 1..start + 1 + end].trim().to_owned();
    }
    cell.split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_matches(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .to_owned()
}

/// Coverage failures.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Donor function definitions.
    pub donor_count: usize,
    /// Matrix rows.
    pub row_count: usize,
    /// Donor functions with no row.
    pub missing: Vec<String>,
    /// Donor functions with more than one row.
    pub duplicates: Vec<String>,
    /// Rows for names not present in the donor.
    pub unknown: Vec<String>,
    /// Rows with an empty Rust mapping.
    pub empty_rust: Vec<String>,
    /// Rows with an empty test.
    pub empty_test: Vec<String>,
    /// Rows with an empty mutation check.
    pub empty_mutation: Vec<String>,
    /// Rows whose status is not `PASS`.
    pub not_pass: Vec<String>,
}

impl Report {
    /// No failure.
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.missing.is_empty()
            && self.duplicates.is_empty()
            && self.unknown.is_empty()
            && self.empty_rust.is_empty()
            && self.empty_test.is_empty()
            && self.empty_mutation.is_empty()
            && self.not_pass.is_empty()
    }

    /// One human-readable line per failure.
    #[must_use]
    pub fn errors(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if !self.missing.is_empty() {
            errors.push(format!(
                "missing ({}): {}",
                self.missing.len(),
                self.missing.join(", ")
            ));
        }
        if !self.duplicates.is_empty() {
            errors.push(format!(
                "duplicate ({}): {}",
                self.duplicates.len(),
                self.duplicates.join(", ")
            ));
        }
        if !self.unknown.is_empty() {
            errors.push(format!(
                "unknown ({}): {}",
                self.unknown.len(),
                self.unknown.join(", ")
            ));
        }
        if !self.empty_rust.is_empty() {
            errors.push(format!(
                "empty Rust ({}): {}",
                self.empty_rust.len(),
                self.empty_rust.join(", ")
            ));
        }
        if !self.empty_test.is_empty() {
            errors.push(format!(
                "empty test ({}): {}",
                self.empty_test.len(),
                self.empty_test.join(", ")
            ));
        }
        if !self.empty_mutation.is_empty() {
            errors.push(format!(
                "empty mutation ({}): {}",
                self.empty_mutation.len(),
                self.empty_mutation.join(", ")
            ));
        }
        if !self.not_pass.is_empty() {
            errors.push(format!(
                "not PASS ({}): {}",
                self.not_pass.len(),
                self.not_pass.join(", ")
            ));
        }
        errors
    }
}

/// Validate a matrix against the donor function definitions.
#[must_use]
pub fn check(donor: &[Function], rows: &[Row]) -> Report {
    let donor_names: Vec<&str> = donor
        .iter()
        .map(|function| function.name.as_str())
        .collect();
    let mut counts = std::collections::BTreeMap::new();
    for row in rows {
        *counts.entry(row.function.as_str()).or_insert(0usize) += 1;
    }
    let mut missing = Vec::new();
    for function in donor {
        if !counts.contains_key(function.name.as_str()) {
            missing.push(function.name.clone());
        }
    }
    let mut duplicates: Vec<String> = counts
        .iter()
        .filter(|(_, count)| **count > 1)
        .map(|(name, _)| (*name).to_owned())
        .collect();
    duplicates.sort();
    let mut unknown: Vec<String> = counts
        .keys()
        .filter(|name| !donor_names.contains(*name))
        .map(|name| (*name).to_owned())
        .collect();
    unknown.sort();
    let empty = |value: &str| {
        let value = value.trim().to_ascii_uppercase();
        value.is_empty()
            || matches!(
                value.as_str(),
                "-" | "—"
                    | "GAP"
                    | "UNMAPPED"
                    | "N/A"
                    | "NA"
                    | "TODO"
                    | "NONE"
                    | "<NONE>"
                    | "BLOCKED"
            )
    };
    let mut empty_rust = Vec::new();
    let mut empty_test = Vec::new();
    let mut empty_mutation = Vec::new();
    let mut not_pass = Vec::new();
    for row in rows {
        if empty(&row.rust) {
            empty_rust.push(row.function.clone());
        }
        if empty(&row.test) {
            empty_test.push(row.function.clone());
        }
        if empty(&row.mutation) {
            empty_mutation.push(row.function.clone());
        }
        if !row.status.trim().eq_ignore_ascii_case("PASS") {
            not_pass.push(row.function.clone());
        }
    }
    empty_rust.sort();
    empty_test.sort();
    empty_mutation.sort();
    not_pass.sort();
    Report {
        donor_count: donor.len(),
        row_count: rows.len(),
        missing,
        duplicates,
        unknown,
        empty_rust,
        empty_test,
        empty_mutation,
        not_pass,
    }
}

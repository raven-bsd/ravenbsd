// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Union audit over the RACK group reports using the K315 checker.
//!
//! K315 validates one matrix at a time. The full port is split across K269 and K328-K336, so
//! this module merges every group report into one union row set, chooses the strongest row per
//! donor function and runs `rack_coverage::check` on it. A function claimed by two groups with a
//! substantive mapping is reported as a cross-report duplicate.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rack_coverage::{Function, Row, check, parse_manifest, parse_matrix};

/// Group reports that make up the RACK port union.
pub const GROUP_REPORTS: &[&str] = &[
    "K269", "K328", "K329", "K330", "K331", "K332", "K333", "K334", "K335", "K336",
];

/// One merged report row with its owning group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedRow {
    /// Group report ID (`K269`, ...).
    pub group: String,
    /// Coverage row.
    pub row: Row,
}

/// Union audit result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnionReport {
    /// Group reports that actually contributed at least one row.
    pub groups: Vec<String>,
    /// Donor function count from the pinned manifest.
    pub donor_count: usize,
    /// Selected union rows.
    pub row_count: usize,
    /// Donor functions with no row.
    pub missing: Vec<String>,
    /// Duplicate names inside the selected union (normally empty).
    pub duplicates: Vec<String>,
    /// Names not present in the donor manifest.
    pub unknown: Vec<String>,
    /// Rows with an empty Rust mapping.
    pub empty_rust: Vec<String>,
    /// Rows with an empty test.
    pub empty_test: Vec<String>,
    /// Rows with an empty mutation/negative check.
    pub empty_mutation: Vec<String>,
    /// Rows not marked `PASS`.
    pub not_pass: Vec<String>,
    /// Functions claimed by more than one group with a substantive mapping.
    pub cross_report_duplicates: Vec<String>,
}

impl UnionReport {
    /// The union is acceptable only when every donor function is present exactly once, every row
    /// is `PASS`, and no two groups claim the same function.
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.missing.is_empty()
            && self.duplicates.is_empty()
            && self.unknown.is_empty()
            && self.empty_rust.is_empty()
            && self.empty_test.is_empty()
            && self.empty_mutation.is_empty()
            && self.not_pass.is_empty()
            && self.cross_report_duplicates.is_empty()
    }

    /// Render the audit as stable text.
    #[must_use]
    pub fn render(&self) -> String {
        let mut text = String::new();
        text.push_str(&format!(
            "union: donors {} rows {} groups {}\n",
            self.donor_count,
            self.row_count,
            self.groups.join(",")
        ));
        text.push_str(&format!(
            "missing={} duplicate={} unknown={} empty_rust={} empty_test={} empty_mutation={} \
             not_pass={} cross_report_duplicate={}\n",
            self.missing.len(),
            self.duplicates.len(),
            self.unknown.len(),
            self.empty_rust.len(),
            self.empty_test.len(),
            self.empty_mutation.len(),
            self.not_pass.len(),
            self.cross_report_duplicates.len()
        ));
        for (label, values) in [
            ("missing", &self.missing),
            ("duplicate", &self.duplicates),
            ("unknown", &self.unknown),
            ("empty Rust", &self.empty_rust),
            ("empty test", &self.empty_test),
            ("empty mutation", &self.empty_mutation),
            ("not PASS", &self.not_pass),
            ("cross-report duplicate", &self.cross_report_duplicates),
        ] {
            if !values.is_empty() {
                text.push_str(&format!(
                    "{label} ({}): {}\n",
                    values.len(),
                    values.join(", ")
                ));
            }
        }
        text
    }
}

/// Path to the pinned 219-function manifest.
#[must_use]
pub fn manifest_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../rack-coverage/fixtures/rack-functions.txt")
}

/// Load the pinned donor function manifest.
pub fn load_manifest() -> Result<Vec<Function>, String> {
    let path = manifest_path();
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    Ok(parse_manifest(&text))
}

/// Load the named group reports from a `tickets` directory.
///
/// A missing report is skipped: a group that has not produced a coverage matrix contributes no
/// rows, and the missing donor functions are reported by the union check.
pub fn load_group_reports(root: &Path) -> Vec<(String, String)> {
    let mut reports = Vec::new();
    for id in GROUP_REPORTS {
        let path = find_report(root, id);
        if let Some(path) = path
            && let Ok(text) = std::fs::read_to_string(&path)
        {
            reports.push(((*id).to_owned(), text));
        }
    }
    reports
}

fn find_report(root: &Path, id: &str) -> Option<PathBuf> {
    let direct = root.join(format!("{id}.md"));
    if direct.is_file() {
        return Some(direct);
    }
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let reports = entry.path().join("reports").join(format!("{id}.md"));
        if reports.is_file() {
            return Some(reports);
        }
    }
    None
}

/// Merge the group reports and run the K315 checker.
#[must_use]
pub fn audit(reports: &[(String, String)], donor: &[Function]) -> UnionReport {
    let mut owned: Vec<OwnedRow> = Vec::new();
    for (group, text) in reports {
        for block in markdown_tables(text) {
            for row in parse_matrix(&block) {
                owned.push(OwnedRow {
                    group: group.clone(),
                    row,
                });
            }
        }
    }

    let mut by_function: BTreeMap<String, Vec<OwnedRow>> = BTreeMap::new();
    for row in owned {
        by_function
            .entry(row.row.function.clone())
            .or_default()
            .push(row);
    }

    let mut cross_report_duplicates = Vec::new();
    let mut selected = Vec::new();
    for (function, mut rows) in by_function {
        let substantive: Vec<&OwnedRow> =
            rows.iter().filter(|row| is_substantive(&row.row)).collect();
        let groups: std::collections::BTreeSet<&str> =
            substantive.iter().map(|row| row.group.as_str()).collect();
        if groups.len() > 1 {
            cross_report_duplicates.push(function.clone());
        }
        rows.sort_by_key(|row| row_score(&row.row));
        if let Some(best) = rows.pop() {
            selected.push(best.row);
        }
    }
    cross_report_duplicates.sort();

    let checked = check(donor, &selected);
    let groups = reports.iter().map(|(id, _)| id.clone()).collect();
    UnionReport {
        groups,
        donor_count: donor.len(),
        row_count: selected.len(),
        missing: checked.missing,
        duplicates: checked.duplicates,
        unknown: checked.unknown,
        empty_rust: checked.empty_rust,
        empty_test: checked.empty_test,
        empty_mutation: checked.empty_mutation,
        not_pass: checked.not_pass,
        cross_report_duplicates,
    }
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

fn is_substantive(row: &Row) -> bool {
    let placeholder = |value: &str| {
        let value = value.trim().to_ascii_uppercase();
        value.is_empty()
            || matches!(
                value.as_str(),
                "-" | "—" | "GAP" | "UNMAPPED" | "N/A" | "TODO" | "BLOCKED"
            )
    };
    !placeholder(&row.rust) && !placeholder(&row.test) && !placeholder(&row.mutation)
}

fn row_score(row: &Row) -> (u8, u8) {
    let pass = u8::from(row.status.eq_ignore_ascii_case("PASS"));
    let mapped = u8::from(is_substantive(row));
    (pass, mapped)
}

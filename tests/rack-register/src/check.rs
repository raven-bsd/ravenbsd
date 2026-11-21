// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Register contract checker: 219 rows, one owner each, non-empty fields, PASS and source
//! existence.

use std::collections::{BTreeMap, BTreeSet};

use rack_acceptance::register::RegisterRow;
use rack_coverage::Function;

use crate::source::SourceIndex;

/// One register failure class.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CheckReport {
    /// Donor functions.
    pub donor: usize,
    /// Register rows.
    pub rows: usize,
    /// Donor functions with no row.
    pub missing: Vec<String>,
    /// Rows for names not in the donor.
    pub unknown: Vec<String>,
    /// Duplicate function rows.
    pub duplicates: Vec<String>,
    /// Rows without a single `Kxxx` owner.
    pub bad_owner: Vec<String>,
    /// Rows with an empty Rust mapping.
    pub empty_rust: Vec<String>,
    /// Rows with an empty test.
    pub empty_test: Vec<String>,
    /// Rows with an empty mutation.
    pub empty_mutation: Vec<String>,
    /// Rows not marked PASS.
    pub not_pass: Vec<String>,
    /// Rust symbols that do not exist in the product source.
    pub missing_symbol: Vec<String>,
    /// Tests that do not exist in the product source.
    pub missing_test: Vec<String>,
}

impl CheckReport {
    /// Whether the register satisfies every contract clause.
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.donor == self.rows
            && self.missing.is_empty()
            && self.unknown.is_empty()
            && self.duplicates.is_empty()
            && self.bad_owner.is_empty()
            && self.empty_rust.is_empty()
            && self.empty_test.is_empty()
            && self.empty_mutation.is_empty()
            && self.not_pass.is_empty()
            && self.missing_symbol.is_empty()
            && self.missing_test.is_empty()
    }

    /// Render the result.
    #[must_use]
    pub fn render(&self) -> String {
        let mut text = format!(
            "register: donors={} rows={} missing={} unknown={} duplicate={} bad_owner={} \
             empty_rust={} empty_test={} empty_mutation={} not_pass={} missing_symbol={} \
             missing_test={}\n",
            self.donor,
            self.rows,
            self.missing.len(),
            self.unknown.len(),
            self.duplicates.len(),
            self.bad_owner.len(),
            self.empty_rust.len(),
            self.empty_test.len(),
            self.empty_mutation.len(),
            self.not_pass.len(),
            self.missing_symbol.len(),
            self.missing_test.len()
        );
        for (label, values) in [
            ("missing", &self.missing),
            ("unknown", &self.unknown),
            ("duplicate", &self.duplicates),
            ("bad owner", &self.bad_owner),
            ("empty Rust", &self.empty_rust),
            ("empty test", &self.empty_test),
            ("empty mutation", &self.empty_mutation),
            ("not PASS", &self.not_pass),
            ("missing symbol", &self.missing_symbol),
            ("missing test", &self.missing_test),
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

/// Run the register checker.
#[must_use]
pub fn check(donor: &[Function], rows: &[RegisterRow], source: &SourceIndex) -> CheckReport {
    let donor_names: BTreeSet<&str> = donor
        .iter()
        .map(|function| function.name.as_str())
        .collect();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for row in rows {
        *counts.entry(row.function.as_str()).or_default() += 1;
    }
    let missing = donor
        .iter()
        .filter(|function| !counts.contains_key(function.name.as_str()))
        .map(|function| function.name.clone())
        .collect();
    let mut unknown: Vec<String> = counts
        .keys()
        .filter(|name| !donor_names.contains(**name))
        .map(|name| (*name).to_owned())
        .collect();
    unknown.sort();
    let mut duplicates: Vec<String> = counts
        .iter()
        .filter(|(_, count)| **count > 1)
        .map(|(name, _)| (*name).to_owned())
        .collect();
    duplicates.sort();

    let mut bad_owner = Vec::new();
    let mut empty_rust = Vec::new();
    let mut empty_test = Vec::new();
    let mut empty_mutation = Vec::new();
    let mut not_pass = Vec::new();
    let mut missing_symbol = Vec::new();
    let mut missing_test = Vec::new();
    for row in rows {
        if !single_owner(&row.owner) {
            bad_owner.push(row.function.clone());
        }
        if placeholder(&row.rust) {
            empty_rust.push(row.function.clone());
        } else if !symbol_exists(source, &row.rust) {
            missing_symbol.push(row.function.clone());
        }
        if placeholder(&row.test) {
            empty_test.push(row.function.clone());
        } else if !test_exists(source, &row.test) {
            missing_test.push(row.function.clone());
        }
        if placeholder(&row.mutation) {
            empty_mutation.push(row.function.clone());
        }
        if !row.status.trim().eq_ignore_ascii_case("PASS") {
            not_pass.push(row.function.clone());
        }
    }

    CheckReport {
        donor: donor.len(),
        rows: rows.len(),
        missing,
        unknown,
        duplicates,
        bad_owner,
        empty_rust,
        empty_test,
        empty_mutation,
        not_pass,
        missing_symbol,
        missing_test,
    }
}

fn single_owner(owner: &str) -> bool {
    let tokens: Vec<&str> = owner.split_whitespace().collect();
    tokens.len() == 1
        && tokens[0].starts_with('K')
        && tokens[0][1..]
            .chars()
            .all(|character| character.is_ascii_digit())
}

fn placeholder(value: &str) -> bool {
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
                | "ASSIGNED"
        )
}

fn symbol_exists(source: &SourceIndex, cell: &str) -> bool {
    cell.split([',', ';', '+'])
        .filter_map(symbol_name)
        .any(|name| source.has_function(&name))
}

fn symbol_name(cell: &str) -> Option<String> {
    let cell = cell.trim().trim_matches('`');
    let cell = cell.split('(').next().unwrap_or(cell);
    let name = cell
        .split([':', '.', '/', ' ', '\t'])
        .rfind(|part| !part.is_empty())?;
    if name
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        Some(name.to_owned())
    } else {
        None
    }
}

fn test_exists(source: &SourceIndex, cell: &str) -> bool {
    cell.split([',', ';'])
        .map(|name| name.trim().trim_matches('`').trim())
        .filter(|name| !name.is_empty())
        .any(|name| source.has_test_named(name))
}

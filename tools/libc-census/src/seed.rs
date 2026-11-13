// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `seed-assignments`: turn the human tables of doc 02 section 3 into candidate
//! `assignments.toml` rows for the reviewer.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use anyhow::{Context, Result};
use regex::Regex;

use crate::model::{Assignment, Assignments, Ring};

/// Rows recognised in the doc 02 tables.
static ROW_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[IPXLBSZ][0-9]{2}$").expect("row id regex"));

static BACKTICK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"`([^`]+)`").expect("backtick regex"));

static IDENT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[\x41-\x5aa-z_][\x41-\x5aa-z0-9_]*$").expect("identifier regex")
});

static DOC_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([0-9]+)").expect("doc regex"));

/// What seeding produced.
#[derive(Clone, Debug, Default)]
pub struct SeedResult {
    /// The candidate assignments, sorted by name.
    pub assignments: Assignments,
    /// Backtick tokens skipped because they are patterns.
    pub skipped_patterns: usize,
    /// Duplicate-name messages.
    pub conflicts: Vec<String>,
    /// Number of table rows read.
    pub rows: usize,
}

/// Parse the doc 02 section 3 tables.
pub fn seed_from_text(text: &str) -> SeedResult {
    let mut result = SeedResult::default();
    let mut by_name: BTreeMap<String, Assignment> = BTreeMap::new();
    let mut first_row: BTreeMap<String, String> = BTreeMap::new();

    for line in text.lines() {
        if !line.starts_with("| ") {
            continue;
        }
        let mut cells: Vec<String> = line
            .split('|')
            .map(str::trim)
            .skip(1)
            .map(str::to_string)
            .collect();
        if cells.last().is_some_and(|cell| cell.is_empty()) {
            cells.pop();
        }
        if cells.len() < 8 {
            continue;
        }
        let id = &cells[0];
        if !ROW_ID_RE.is_match(id) {
            continue;
        }
        result.rows += 1;
        let ring_cell = &cells[3];
        let (ring, override_reason) = parse_ring(ring_cell, id);
        let personality = match cells[4].as_str() {
            "-" | "" => None,
            value => Some(value.to_string()),
        };
        let area = DOC_RE
            .captures(&cells[5])
            .and_then(|capture| capture.get(1))
            .and_then(|value| value.as_str().parse::<u32>().ok());
        let crate_name = clean_crate(&cells[6]);
        let status = match cells[7].as_str() {
            "-" | "" => None,
            value => Some(value.to_string()),
        };

        let mut tokens: Vec<String> = Vec::new();
        for capture in BACKTICK_RE.captures_iter(&cells[2]) {
            if let Some(group) = capture.get(1) {
                for token in group.as_str().split_whitespace() {
                    tokens.push(token.to_string());
                }
            }
        }
        for token in tokens {
            if token.contains('*')
                || token.contains("...")
                || token.contains('/')
                || token.starts_with('<')
            {
                result.skipped_patterns += 1;
                continue;
            }
            if !IDENT_RE.is_match(&token) || token.len() < 2 {
                continue;
            }
            if let Some(previous) = first_row.get(&token) {
                result.conflicts.push(format!(
                    "{token} appears in {previous} and {id}; keeping {previous}"
                ));
                continue;
            }
            first_row.insert(token.clone(), id.clone());
            by_name.insert(
                token.clone(),
                Assignment {
                    name: token,
                    ring: Some(ring.clone()),
                    personality: personality.clone(),
                    crate_name: crate_name.clone(),
                    area,
                    status: status.clone(),
                    header: None,
                    wave: None,
                    warn: None,
                    stub: None,
                    gap: Vec::new(),
                    ring_override: override_reason.clone(),
                    kind: None,
                    aliases: Vec::new(),
                },
            );
        }
    }

    result.assignments = Assignments {
        schema: Some(1),
        assign: by_name.into_values().collect(),
    };
    result
}

fn parse_ring(cell: &str, id: &str) -> (Ring, Option<String>) {
    if cell.starts_with("private") {
        let reason = cell.contains('/').then(|| format!("mixed row {id}"));
        return (Ring::private(), reason);
    }
    let number = cell
        .chars()
        .next()
        .and_then(|character| character.to_digit(10))
        .unwrap_or(0) as u8;
    let reason = cell.contains('/').then(|| format!("mixed row {id}"));
    (Ring::Number(number), reason)
}

fn clean_crate(cell: &str) -> Option<String> {
    let cell = cell.trim();
    if cell.is_empty() || cell == "-" {
        return None;
    }
    let token = cell
        .split(|character: char| character.is_whitespace() || matches!(character, ',' | '(' | ')'))
        .find(|token| !token.is_empty())?;
    Some(token.to_string())
}

/// Parse, write and return the seeded assignments.
pub fn seed(api_surface: &Path, out: &Path) -> Result<SeedResult> {
    let text = fs::read_to_string(api_surface)
        .with_context(|| format!("reading {}", api_surface.display()))?;
    let result = seed_from_text(&text);
    let mut output = String::new();
    output.push_str("# SPDX-License-Identifier: BSD-3-Clause\n");
    output.push_str("# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors\n");
    output.push_str("# SEEDED by libc-census seed-assignments; review before merge.\n");
    output.push_str("schema = 1\n");
    for assignment in &result.assignments.assign {
        output.push_str("\n[[assign]]\n");
        output.push_str(
            &toml::to_string(assignment)
                .with_context(|| format!("serialising assignment {}", assignment.name))?,
        );
    }
    if let Some(parent) = out.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(out, output).with_context(|| format!("writing {}", out.display()))?;
    Ok(result)
}

/// Distinct names in a seed result, for tests and summaries.
pub fn names(result: &SeedResult) -> BTreeSet<&str> {
    result
        .assignments
        .assign
        .iter()
        .map(|assignment| assignment.name.as_str())
        .collect()
}

#[cfg(test)]
#[path = "seed_ut.rs"]
mod tests;

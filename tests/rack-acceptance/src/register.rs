// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Canonical K315 register and pinned donor-manifest loading.
//!
//! `docs/rack-coverage.md` is the owner map: every donor function appears once with its owning
//! group. The pinned manifest is the FreeBSD 219-function list.

use std::path::{Path, PathBuf};

use rack_coverage::{Function, parse_manifest};

/// One row of the canonical union register.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisterRow {
    /// Donor line number.
    pub number: usize,
    /// Donor function name.
    pub function: String,
    /// Owning ticket (`K269`, `K328`, ...).
    pub owner: String,
    /// Rust mapping claimed by the register.
    pub rust: String,
    /// Named test claimed by the register.
    pub test: String,
    /// Mutation/negative check claimed by the register.
    pub mutation: String,
    /// Register status (`PASS` or `ASSIGNED Kxxx`).
    pub status: String,
}

/// Repository root from this crate's manifest directory.
#[must_use]
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Canonical register path.
#[must_use]
pub fn register_path() -> PathBuf {
    repo_root().join("docs/rack-coverage.md")
}

/// Pinned donor manifest path.
#[must_use]
pub fn manifest_path() -> PathBuf {
    repo_root().join("tests/rack-coverage/fixtures/rack-functions.txt")
}

/// Load the pinned donor function list.
pub fn load_donor() -> Result<Vec<Function>, String> {
    let path = manifest_path();
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    Ok(parse_manifest(&text))
}

/// Load the canonical register from the repository.
pub fn load_register() -> Result<Vec<RegisterRow>, String> {
    let path = register_path();
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    parse_register(&text).map_err(|error| format!("{}: {error}", path.display()))
}

/// Parse the canonical register table.
pub fn parse_register(text: &str) -> Result<Vec<RegisterRow>, String> {
    let mut rows = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = trimmed
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        if cells.len() < 7 {
            continue;
        }
        let Ok(number) = cells[0].parse::<usize>() else {
            continue;
        };
        if !cells[2].starts_with('K') {
            continue;
        }
        let Some(function) = backticked(cells[1]) else {
            continue;
        };
        rows.push(RegisterRow {
            number,
            function,
            owner: cells[2].to_owned(),
            rust: clean(cells[3]),
            test: clean(cells[4]),
            mutation: clean(cells[5]),
            status: cells[6].to_owned(),
        });
        if rows.len() > 1_000 {
            return Err(format!("line {}: register does not converge", index + 1));
        }
    }
    if rows.is_empty() {
        return Err("no register rows found".to_owned());
    }
    Ok(rows)
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

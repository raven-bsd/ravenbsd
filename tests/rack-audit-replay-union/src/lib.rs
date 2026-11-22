// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Independent audit of the merged RACK replay and union evidence.
//!
//! This crate reads the maintained `docs/rack-coverage.md` register and the live group reports,
//! runs the K315 checker over both views, checks that every PASS row has a merged Rust function,
//! and records the donor-constant discrepancies. Findings are data, not hidden test failures.

#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};

use rack_coverage::{Function, Report, Row, check, parse_matrix};
use rack_replay::union::{self, UnionReport};

/// Default management-tree path for the live group reports.
pub const DEFAULT_TICKETS: &str = "/home/bo/work/ravenos-workspace/ravenbsd-prjmgmt/tickets";

/// One independent audit snapshot.
#[derive(Clone, Debug)]
pub struct Audit {
    /// Donor functions in the pinned manifest.
    pub donor_count: usize,
    /// Rows in `docs/rack-coverage.md`.
    pub register_rows: usize,
    /// K315 result over the register.
    pub register_check: Report,
    /// K315 result over the live group-report union.
    pub union: UnionReport,
    /// Register rows marked `PASS`.
    pub pass_rows: usize,
    /// PASS rows whose Rust function could not be found in the merged crate.
    pub missing_rust: Vec<String>,
    /// Exact donor-constant discrepancies.
    pub constant_findings: Vec<String>,
}

/// Repository root from this test package.
#[must_use]
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Load the maintained register rows.
///
/// # Errors
///
/// Returns a message when the register cannot be read.
pub fn load_register(root: &Path) -> Result<Vec<Row>, String> {
    let path = root.join("docs/rack-coverage.md");
    let text = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(parse_matrix(&text))
}

/// Read all merged `raven-tcpip` source text.
///
/// # Errors
///
/// Returns a message when the source directory cannot be read.
pub fn merged_rust(root: &Path) -> Result<String, String> {
    let directory = root.join("crates/raven-tcpip/src");
    let mut text = String::new();
    let entries =
        fs::read_dir(&directory).map_err(|error| format!("{}: {error}", directory.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|extension| extension == "rs")
            && let Ok(source) = fs::read_to_string(&path)
        {
            text.push_str(&source);
            text.push('\n');
        }
    }
    Ok(text)
}

/// PASS rows whose last Rust path segment is not a `fn` in the merged crate.
#[must_use]
pub fn missing_rust(rows: &[Row], source: &str) -> Vec<String> {
    let mut missing = Vec::new();
    for row in rows {
        if !row.status.eq_ignore_ascii_case("PASS") {
            continue;
        }
        let name = row.rust.rsplit("::").next().unwrap_or("").trim();
        if name.is_empty() || name == "-" || !source.contains(&format!("fn {name}")) {
            missing.push(format!("{} -> {}", row.function, row.rust));
        }
    }
    missing
}

/// Exact donor-constant discrepancies found in the merged vtable module.
#[must_use]
pub fn constant_findings() -> Vec<String> {
    let mut findings = Vec::new();
    if raven_tcpip::TCP_FUNC_OUTPUT_CANDROP != 0x02 {
        findings.push(format!(
            "TCP_FUNC_OUTPUT_CANDROP: donor 0x02, merged 0x{:02x}",
            raven_tcpip::TCP_FUNC_OUTPUT_CANDROP
        ));
    }
    if raven_tcpip::TCP_FUNC_DEFAULT_OK != 0x04 {
        findings.push(format!(
            "TCP_FUNC_DEFAULT_OK: donor 0x04, merged 0x{:02x}",
            raven_tcpip::TCP_FUNC_DEFAULT_OK
        ));
    }
    if raven_tcpip::RACK_FREE_CNT_MAX != 0x2f {
        findings.push(format!(
            "RACK_FREE_CNT_MAX: donor 0x2f, merged {}",
            raven_tcpip::RACK_FREE_CNT_MAX
        ));
    }
    findings
}

/// Build the full audit snapshot.
///
/// # Errors
///
/// Returns a message when a manifest, register or source read fails.
pub fn audit(root: &Path, tickets: &Path) -> Result<Audit, String> {
    let donor: Vec<Function> = union::load_manifest()?;
    let rows = load_register(root)?;
    let register_check = check(&donor, &rows);
    let reports = union::load_group_reports(tickets);
    let union = union::audit(&reports, &donor);
    let source = merged_rust(root)?;
    let pass_rows = rows
        .iter()
        .filter(|row| row.status.eq_ignore_ascii_case("PASS"))
        .count();
    Ok(Audit {
        donor_count: donor.len(),
        register_rows: rows.len(),
        register_check,
        union,
        pass_rows,
        missing_rust: missing_rust(&rows, &source),
        constant_findings: constant_findings(),
    })
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

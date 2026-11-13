// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Merge the run outputs and `assignments.toml` into the committed census data.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::model::{
    Assignment, Assignments, CellRow, Kind, PortsOutput, Ring, RunSummary, StandardEntry,
    StandardsOutput, SymbolRow, UsesOutput,
};
use crate::standards::{compute_ring, decide_ring};

/// Options for `merge`.
#[derive(Clone, Debug)]
pub struct MergeOptions {
    /// Directory with the `run` outputs.
    pub out: PathBuf,
    /// Directory receiving the committed census files.
    pub census: PathBuf,
    /// Assignment file; defaults to `<census>/assignments.toml`.
    pub assignments: Option<PathBuf>,
}

impl Default for MergeOptions {
    fn default() -> Self {
        Self {
            out: PathBuf::from("tools/libc-census/out"),
            census: PathBuf::from("libc/ravenlibc/abi/census"),
            assignments: None,
        }
    }
}

/// What `merge` wrote.
#[derive(Clone, Debug, Default)]
pub struct MergeReport {
    /// Number of `[[symbol]]` rows.
    pub symbols: usize,
    /// Ring histogram.
    pub rings: BTreeMap<String, usize>,
    /// Number of `ring_diff.md` data rows.
    pub ring_diff: usize,
    /// Whether every donor and root was present.
    pub complete: bool,
    /// The rows, for tests and the CLI summary.
    pub rows: Vec<SymbolRow>,
}

/// The census dashboard JSON.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Census {
    /// Schema version.
    pub schema: u32,
    /// Revision stamp.
    pub census_rev: String,
    /// Every donor and root present.
    pub complete: bool,
    /// Counts per ring.
    pub rings: BTreeMap<String, usize>,
    /// Counts per `n6`.
    pub n6: BTreeMap<String, usize>,
    /// Names with no standard and `n6 >= 2`.
    pub ring1_candidates: Vec<String>,
    /// Top symbol call sites, at most 100.
    pub top_symbols: Vec<CountRow>,
    /// Top headers, at most 50.
    pub top_headers: Vec<CountRow>,
    /// Libraries with package counts.
    pub libraries: Vec<LibraryRow>,
    /// Absent names grouped by assignment area.
    pub areas: BTreeMap<String, Vec<String>>,
}

/// One count row.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CountRow {
    /// Item name.
    pub name: String,
    /// Package count.
    pub packages: usize,
}

/// One library row.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LibraryRow {
    /// Library name without `-l`.
    pub name: String,
    /// Package count.
    pub packages: usize,
    /// Subsumed by ravenlibc and shipped as a stub.
    pub stub: bool,
}

/// Merge and write the census files.
pub fn merge(options: &MergeOptions) -> Result<MergeReport> {
    let cells: Vec<CellRow> = read_optional(&options.out.join("cells.json"))?;
    let standards: StandardsOutput = read_optional(&options.out.join("standards.json"))?;
    let summary: RunSummary = read_optional(&options.out.join("run-summary.json"))?;
    let ports: PortsOutput = read_optional(&options.out.join("ports.json"))?;
    let uses: UsesOutput = read_optional(&options.out.join("uses.json"))?;

    let assignment_path = options
        .assignments
        .clone()
        .unwrap_or_else(|| options.census.join("assignments.toml"));
    let assignments = load_assignments(&assignment_path)?;
    let by_name: BTreeMap<String, Assignment> = assignments
        .assign
        .iter()
        .map(|assignment| (assignment.name.clone(), assignment.clone()))
        .collect();

    let mut names: BTreeSet<String> = cells.iter().map(|row| row.name.clone()).collect();
    names.extend(by_name.keys().cloned());

    let mut rows = Vec::new();
    for name in names {
        let cell = cells.iter().find(|row| row.name == name);
        let standard_entry = standards.entries.get(&name);
        let assignment = by_name.get(&name);
        rows.push(build_row(&name, cell, standard_entry, assignment)?);
    }
    rows.sort_by(|left, right| left.name.cmp(&right.name));

    fs::create_dir_all(&options.census)
        .with_context(|| format!("creating {}", options.census.display()))?;
    write_symbols(&options.census.join("symbols.toml"), &rows, &summary)?;
    let report = report(&rows, &summary);
    write_census(&options.census, &rows, &report, &ports, &uses, &summary)?;
    write_ring_diff(&options.census.join("ring_diff.md"), &rows)?;
    write_patchset_gaps(&options.census.join("patchset-gaps.md"), &ports)?;
    write_known_differences_if_absent(&options.census.join("known-differences.toml"))?;
    Ok(report)
}

fn build_row(
    name: &str,
    cell: Option<&CellRow>,
    standard_entry: Option<&StandardEntry>,
    assignment: Option<&Assignment>,
) -> Result<SymbolRow> {
    let standard: Vec<String> = standard_entry
        .map(|entry| entry.standard.clone())
        .unwrap_or_default();
    let families = cell.map(|row| row.families.clone()).unwrap_or_default();
    let computed = compute_ring(&standard, &families);
    let decision = decide_ring(&computed, assignment)?;
    let assignment_kind = assignment
        .and_then(|assignment| assignment.kind.as_deref())
        .map(Kind::parse)
        .transpose()?;
    let kind = assignment_kind
        .or_else(|| standard_entry.and_then(|entry| Kind::parse(&entry.kind).ok()))
        .or_else(|| cell.map(|row| row.kind))
        .unwrap_or(Kind::Func);
    let header = standard_entry
        .and_then(|entry| entry.header.clone())
        .or_else(|| assignment.and_then(|assignment| assignment.header.clone()));
    let personality = if decision.ring == Ring::Number(2) {
        assignment
            .and_then(|assignment| assignment.personality.clone())
            .or(computed.personality.clone())
    } else {
        None
    };
    let libcs = cell.map(|row| row.libcs.clone()).unwrap_or_default();
    let n6 = cell.and_then(|row| (!row.libcs.is_empty()).then_some(row.n6));
    Ok(SymbolRow {
        name: name.to_string(),
        ring: Some(decision.ring),
        header,
        version: None,
        kind: Some(kind.to_string()),
        standard,
        families,
        personality,
        aliases: assignment
            .map(|assignment| assignment.aliases.clone())
            .unwrap_or_default(),
        crate_name: assignment.and_then(|assignment| assignment.crate_name.clone()),
        area: assignment.and_then(|assignment| assignment.area),
        status: assignment.and_then(|assignment| assignment.status.clone()),
        wave: assignment.and_then(|assignment| assignment.wave),
        warn: assignment.and_then(|assignment| assignment.warn.clone()),
        stub: assignment.and_then(|assignment| assignment.stub.clone()),
        gap: assignment
            .map(|assignment| assignment.gap.clone())
            .unwrap_or_default(),
        libcs,
        n6,
        ring_computed: computed.ring,
        ring_source: decision.source.to_string(),
        ring_override: decision.ring_override,
    })
}

/// Load `assignments.toml`; a missing default file is an empty assignment set.
pub fn load_assignments(path: &Path) -> Result<Assignments> {
    if !path.exists() {
        return Ok(Assignments::default());
    }
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let assignments: Assignments =
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    for assignment in &assignments.assign {
        if let Some(ring) = &assignment.ring {
            ring.validate()?;
        }
    }
    Ok(assignments)
}

fn report(rows: &[SymbolRow], summary: &RunSummary) -> MergeReport {
    let mut rings: BTreeMap<String, usize> = BTreeMap::new();
    let mut ring_diff = 0usize;
    for row in rows {
        let key = match &row.ring {
            Some(Ring::Number(number)) => number.to_string(),
            Some(Ring::Private(_)) => "private".to_string(),
            None => "none".to_string(),
        };
        *rings.entry(key).or_insert(0) += 1;
        if row.ring_source == "assignment"
            && row
                .ring
                .as_ref()
                .and_then(Ring::number)
                .is_some_and(|number| number != row.ring_computed)
        {
            ring_diff += 1;
        }
    }
    MergeReport {
        symbols: rows.len(),
        rings,
        ring_diff,
        complete: summary.complete,
        rows: rows.to_vec(),
    }
}

fn write_symbols(path: &Path, rows: &[SymbolRow], summary: &RunSummary) -> Result<()> {
    let mut text = String::new();
    text.push_str("# SPDX-License-Identifier: BSD-3-Clause\n");
    text.push_str("# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors\n");
    text.push_str("# GENERATED by libc-census; do not edit\n");
    text.push_str("schema = 1\n");
    text.push_str(&format!(
        "census_rev = {}\n",
        serde_json::to_string(&summary.census_rev)
            .unwrap_or_else(|_| "\"unversioned\"".to_string())
    ));
    text.push_str(&format!(
        "complete = {}\n",
        if summary.complete { "true" } else { "false" }
    ));
    text.push_str("[donors]\n");
    for (donor, revision) in &summary.donors {
        text.push_str(&format!(
            "{} = {}\n",
            donor,
            serde_json::to_string(revision).unwrap_or_else(|_| "\"unknown\"".to_string())
        ));
    }
    for row in rows {
        text.push_str("\n[[symbol]]\n");
        text.push_str(
            &toml::to_string(row).with_context(|| format!("serialising symbol {}", row.name))?,
        );
    }
    write_text(path, &text)
}

fn write_census(
    census: &Path,
    rows: &[SymbolRow],
    report: &MergeReport,
    ports: &PortsOutput,
    uses: &UsesOutput,
    summary: &RunSummary,
) -> Result<()> {
    let mut rings = BTreeMap::new();
    for ring in 0..=3u8 {
        rings.insert(
            ring.to_string(),
            *report.rings.get(&ring.to_string()).unwrap_or(&0),
        );
    }
    if let Some(private) = report.rings.get("private") {
        rings.insert("private".to_string(), *private);
    }
    let mut n6 = BTreeMap::new();
    for count in 0..=6u32 {
        n6.insert(
            count.to_string(),
            rows.iter()
                .filter(|row| row.n6.unwrap_or(0) == count)
                .count(),
        );
    }
    let ring1_candidates: Vec<String> = rows
        .iter()
        .filter(|row| row.standard.is_empty() && row.n6.unwrap_or(0) >= 2)
        .map(|row| row.name.clone())
        .collect();
    let symbol_counts = if !ports.symbols.is_empty() {
        ports.symbols.clone()
    } else if let Some(undef) = &uses.undef
        && undef.skipped.is_none()
    {
        let mut counts = undef.strong.clone();
        for (name, count) in &undef.weak {
            let entry = counts.entry(name.clone()).or_insert(0);
            *entry = entry.saturating_add(*count);
        }
        counts
    } else {
        uses.symbols.clone()
    };
    let top_symbols = top_counts(&symbol_counts, 100);
    let top_headers = top_counts(&uses.headers, 50);
    let mut libraries: Vec<LibraryRow> = uses
        .libraries
        .iter()
        .map(|(name, library)| LibraryRow {
            name: name.clone(),
            packages: library.packages,
            stub: library.stub,
        })
        .collect();
    libraries.sort_by(|left, right| {
        right
            .packages
            .cmp(&left.packages)
            .then_with(|| left.name.cmp(&right.name))
    });
    let mut areas: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for row in rows {
        let Some(area) = row.area else {
            continue;
        };
        if !row.libcs.is_empty() {
            continue;
        }
        areas
            .entry(area.to_string())
            .or_default()
            .push(row.name.clone());
    }
    let document = Census {
        schema: 1,
        census_rev: summary.census_rev.clone(),
        complete: summary.complete,
        rings,
        n6,
        ring1_candidates,
        top_symbols,
        top_headers,
        libraries,
        areas,
    };
    write_json(&census.join("census.json"), &document)?;

    let mut text = String::new();
    text.push_str("<!-- SPDX-License-Identifier: BSD-3-Clause -->\n");
    text.push_str("<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->\n");
    text.push_str("<!-- GENERATED by libc-census; do not edit -->\n\n");
    text.push_str("# ravenlibc API census\n\n");
    text.push_str(
        "Generated by `libc-census merge`; facts only. Schema 1, `census_rev` \
         ",
    );
    text.push_str(&format!(
        "`{}`, complete `{}`.\n\n",
        document.census_rev, document.complete
    ));
    text.push_str("## Symbols per ring\n\n| ring | symbols |\n|---|---|\n");
    for (ring, count) in &document.rings {
        text.push_str(&format!("| {ring} | {count} |\n"));
    }
    text.push_str("\n## Symbols per n6\n\n| n6 | symbols |\n|---|---|\n");
    for (n6, count) in &document.n6 {
        text.push_str(&format!("| {n6} | {count} |\n"));
    }
    text.push_str("\n## Ring-1 candidates (n6 >= 2, no standard)\n\n");
    if document.ring1_candidates.is_empty() {
        text.push_str("_None._\n");
    } else {
        for name in &document.ring1_candidates {
            text.push_str(&format!("- `{name}`\n"));
        }
    }
    text.push_str("\n## Ports snapshot\n\n");
    for (snapshot, status) in &ports.snapshots {
        match &status.skipped {
            Some(reason) => text.push_str(&format!("- {snapshot}: `{reason}`\n")),
            None => text.push_str(&format!("- {snapshot}: {} package(s)\n", status.packages)),
        }
    }
    if ports.snapshots.is_empty() {
        text.push_str("- `skipped: no ports snapshot given`\n");
    }
    if uses.skipped.is_some() {
        text.push_str("- sources: `skipped: no sources tree`\n");
    }
    text.push_str("\n## Top symbols\n\n");
    if document.top_symbols.is_empty() {
        text.push_str("_No source or undefined-symbol data._\n");
    } else {
        text.push_str("| # | symbol | packages |\n|---|---|---|\n");
        for (index, row) in document.top_symbols.iter().enumerate() {
            text.push_str(&format!(
                "| {} | `{}` | {} |\n",
                index + 1,
                row.name,
                row.packages
            ));
        }
    }
    text.push_str("\n## Top headers\n\n");
    if document.top_headers.is_empty() {
        text.push_str("_No source data._\n");
    } else {
        text.push_str("| # | header | packages |\n|---|---|---|\n");
        for (index, row) in document.top_headers.iter().enumerate() {
            text.push_str(&format!(
                "| {} | `<{}>` | {} |\n",
                index + 1,
                row.name,
                row.packages
            ));
        }
    }
    text.push_str("\n## Libraries\n\n");
    if document.libraries.is_empty() {
        text.push_str("_No build data._\n");
    } else {
        text.push_str("| library | packages | stub |\n|---|---|---|\n");
        for library in &document.libraries {
            text.push_str(&format!(
                "| `-l{}` | {} | {} |\n",
                library.name,
                library.packages,
                if library.stub { "yes" } else { "no" }
            ));
        }
    }
    text.push_str("\n## Areas with absent names\n\n");
    if document.areas.is_empty() {
        text.push_str("_None._\n");
    } else {
        for (area, names) in &document.areas {
            text.push_str(&format!("### area {area}\n\n"));
            for name in names {
                text.push_str(&format!("- `{name}`\n"));
            }
            text.push('\n');
        }
    }
    write_text(&census.join("census.md"), &text)
}

fn top_counts(counts: &BTreeMap<String, usize>, limit: usize) -> Vec<CountRow> {
    let mut rows: Vec<CountRow> = counts
        .iter()
        .map(|(name, count)| CountRow {
            name: name.clone(),
            packages: *count,
        })
        .collect();
    rows.sort_by(|left, right| {
        right
            .packages
            .cmp(&left.packages)
            .then_with(|| left.name.cmp(&right.name))
    });
    rows.truncate(limit);
    rows
}

fn write_ring_diff(path: &Path, rows: &[SymbolRow]) -> Result<()> {
    let mut text = String::new();
    text.push_str("<!-- SPDX-License-Identifier: BSD-3-Clause -->\n");
    text.push_str("<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->\n");
    text.push_str("<!-- GENERATED by libc-census; do not edit -->\n\n");
    text.push_str("# Ring differences\n\n");
    text.push_str(
        "| name | ring_computed | ring_assignment | ring_override |\n|---|---|---|---|\n",
    );
    let mut count = 0usize;
    for row in rows {
        let Some(number) = row.ring.as_ref().and_then(Ring::number) else {
            continue;
        };
        if row.ring_source != "assignment" || number == row.ring_computed {
            continue;
        }
        count += 1;
        text.push_str(&format!(
            "| `{}` | {} | {} | {} |\n",
            row.name,
            row.ring_computed,
            number,
            row.ring_override.clone().unwrap_or_default()
        ));
    }
    if count == 0 {
        text.push_str("\n_No differences._\n");
    }
    write_text(path, &text)
}

fn write_patchset_gaps(path: &Path, ports: &PortsOutput) -> Result<()> {
    let mut text = String::new();
    text.push_str("<!-- SPDX-License-Identifier: BSD-3-Clause -->\n");
    text.push_str("<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->\n");
    text.push_str("<!-- GENERATED by libc-census; do not edit -->\n\n");
    text.push_str("# Patch-set gaps\n\n");
    if ports.packages.is_empty() && ports.symbols.is_empty() {
        text.push_str("skipped: no ports snapshot given\n");
        return write_text(path, &text);
    }
    for (title, counts) in [
        ("Headers", &ports.headers),
        ("Macros", &ports.macros),
        ("Symbols", &ports.symbols),
    ] {
        text.push_str(&format!("## {title}\n\n"));
        if counts.is_empty() {
            text.push_str("_None._\n\n");
            continue;
        }
        text.push_str("| item | packages |\n|---|---|\n");
        let rows = top_counts(counts, counts.len());
        for row in rows {
            text.push_str(&format!("| `{}` | {} |\n", row.name, row.packages));
        }
        text.push('\n');
    }
    write_text(path, &text)
}

fn write_known_differences_if_absent(path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    let text = "\
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
# Reviewer input for compare-table; libc-census writes this file only when absent.
schema = 1
";
    write_text(path, text)
}

fn read_optional<T>(path: &Path) -> Result<T>
where
    T: Default + serde::de::DeserializeOwned,
{
    if !path.exists() {
        return Ok(T::default());
    }
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut text = serde_json::to_string_pretty(value)
        .with_context(|| format!("serialising {}", path.display()))?;
    text.push('\n');
    write_text(path, &text)
}

fn write_text(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
#[path = "merge_ut.rs"]
mod tests;

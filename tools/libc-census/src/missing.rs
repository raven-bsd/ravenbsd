// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `missing`: rank absent names by package use (ticket 4.6).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::model::{PortsOutput, Ring, UsesOutput};
use crate::standards::is_ring0;
use crate::symlists::parse_openbsd;

/// Options for `missing`.
#[derive(Clone, Debug)]
pub struct MissingOptions {
    /// The implemented-names file.
    pub implemented: PathBuf,
    /// Directory with the run outputs and where `missing.md` is written.
    pub out: PathBuf,
    /// P0 threshold.
    pub min_packages: usize,
    /// Optional explicit `symbols.toml`.
    pub symbols: Option<PathBuf>,
}

impl Default for MissingOptions {
    fn default() -> Self {
        Self {
            implemented: PathBuf::new(),
            out: PathBuf::from("tools/libc-census/out"),
            min_packages: 3,
            symbols: None,
        }
    }
}

/// One class in the report.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Class {
    /// Used by `min_packages` or more and absent.
    P0,
    /// Used by one or two packages and absent.
    P1,
    /// Ring 0 by standard and absent; no package data needed.
    P2,
}

impl Class {
    /// Spelling in the report.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::P0 => "P0",
            Self::P1 => "P1",
            Self::P2 => "P2",
        }
    }
}

/// One ranked row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingRow {
    /// Symbol name.
    pub name: String,
    /// Class.
    pub class: Class,
    /// Package count (0 when no package data).
    pub packages: usize,
}

/// The result of one `missing` run.
#[derive(Clone, Debug, Default)]
pub struct MissingReport {
    /// Rows, P0 then P1 then P2, each sorted by count then name.
    pub rows: Vec<MissingRow>,
    /// Which package data source was used (`undef`, `patches`, `sources` or `none`).
    pub source: String,
    /// Whether P0 is non-empty.
    pub p0_nonempty: bool,
}

/// The generated `symbols.toml` wrapper.
#[derive(Clone, Debug, Default, Deserialize)]
struct SymbolsFile {
    #[serde(default)]
    symbol: Vec<SymbolStub>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct SymbolStub {
    name: String,
    #[serde(default)]
    ring: Option<Ring>,
    #[serde(default)]
    standard: Vec<String>,
}

/// Read a `Symbols.list`-style file (one name per line, C comments allowed).
pub fn read_implemented(path: &Path) -> Result<BTreeSet<String>> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(parse_openbsd(&text).names)
}

/// Run the ranking and write `missing.md`.
pub fn run_missing(options: &MissingOptions) -> Result<MissingReport> {
    let implemented = read_implemented(&options.implemented)?;
    let symbols_path = options.symbols.clone().unwrap_or_else(|| {
        let local = options.out.join("symbols.toml");
        if local.exists() {
            local
        } else {
            PathBuf::from("libc/ravenlibc/abi/census/symbols.toml")
        }
    });
    let symbols: SymbolsFile = if symbols_path.exists() {
        let text = fs::read_to_string(&symbols_path)
            .with_context(|| format!("reading {}", symbols_path.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing {}", symbols_path.display()))?
    } else {
        SymbolsFile::default()
    };

    let ports: PortsOutput = read_json(&options.out.join("ports.json"))?;
    let uses: UsesOutput = read_json(&options.out.join("uses.json"))?;
    let (counts, source) = package_counts(&ports, &uses);

    let mut rows = Vec::new();
    for symbol in &symbols.symbol {
        if implemented.contains(&symbol.name) {
            continue;
        }
        let count = counts.get(&symbol.name).copied().unwrap_or(0);
        let class = if source != "none" && count >= options.min_packages {
            Class::P0
        } else if source != "none" && count >= 1 {
            Class::P1
        } else if is_ring0(&symbol.standard)
            && symbol.ring.as_ref().and_then(Ring::number) != Some(3)
        {
            Class::P2
        } else {
            continue;
        };
        rows.push(MissingRow {
            name: symbol.name.clone(),
            class,
            packages: count,
        });
    }
    rows.sort_by(|left, right| {
        left.class
            .cmp(&right.class)
            .then_with(|| right.packages.cmp(&left.packages))
            .then_with(|| left.name.cmp(&right.name))
    });
    let p0_nonempty = rows.iter().any(|row| row.class == Class::P0);
    let report = MissingReport {
        rows,
        source: source.to_string(),
        p0_nonempty,
    };
    write_report(&options.out.join("missing.md"), &report, options)?;
    Ok(report)
}

fn package_counts(
    ports: &PortsOutput,
    uses: &UsesOutput,
) -> (BTreeMap<String, usize>, &'static str) {
    if let Some(undef) = &uses.undef
        && undef.skipped.is_none()
    {
        let mut counts: BTreeMap<String, usize> = undef.strong.clone();
        for (name, count) in &undef.weak {
            let entry = counts.entry(name.clone()).or_insert(0);
            *entry = entry.saturating_add(*count);
        }
        return (counts, "undef");
    }
    let has_ports = ports
        .snapshots
        .values()
        .any(|snapshot| snapshot.skipped.is_none());
    if has_ports || !ports.symbols.is_empty() {
        return (ports.symbols.clone(), "patches");
    }
    if uses.skipped.is_none() && uses.packages > 0 {
        return (uses.symbols.clone(), "sources");
    }
    (BTreeMap::new(), "none")
}

fn write_report(path: &Path, report: &MissingReport, options: &MissingOptions) -> Result<()> {
    let mut text = String::new();
    text.push_str("<!-- SPDX-License-Identifier: BSD-3-Clause -->\n");
    text.push_str("<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->\n");
    text.push_str("<!-- GENERATED by libc-census; do not edit -->\n\n");
    text.push_str("# Missing ravenlibc symbols\n\n");
    text.push_str(&format!("Package data: `{}`.\n", report.source));
    text.push_str(&format!(
        "P0 threshold: {} package(s).\n\n",
        options.min_packages
    ));
    if report.source == "none" {
        text.push_str("P0/P1 need a ports snapshot.\n\n");
        text.push_str("## P2\n\n");
        append_rows(&mut text, report, Class::P2);
    } else {
        for class in [Class::P0, Class::P1, Class::P2] {
            text.push_str(&format!("## {}\n\n", class.as_str()));
            append_rows(&mut text, report, class);
        }
    }
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

fn append_rows(text: &mut String, report: &MissingReport, class: Class) {
    let rows: Vec<&MissingRow> = report
        .rows
        .iter()
        .filter(|row| row.class == class)
        .collect();
    if rows.is_empty() {
        text.push_str("_None._\n\n");
        return;
    }
    text.push_str("| symbol | packages |\n|---|---|\n");
    for row in rows {
        text.push_str(&format!("| `{}` | {} |\n", row.name, row.packages));
    }
    text.push('\n');
}

fn read_json<T>(path: &Path) -> Result<T>
where
    T: Default + serde::de::DeserializeOwned,
{
    if !path.exists() {
        return Ok(T::default());
    }
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

#[cfg(test)]
#[path = "missing_ut.rs"]
mod tests;

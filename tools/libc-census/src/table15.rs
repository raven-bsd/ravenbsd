// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `compare-table`: the acceptance check against doc 15 section 3.B.4.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use anyhow::{Context, Result, bail};
use regex::Regex;

use crate::cells::{ScanOptions, cell_rows, donor_present, donor_roots, scan_donors};
use crate::model::{Exit, KnownDifferences};
use crate::universe::Universe;

/// The header tolerance from ticket 4.7.
pub const HEADER_TOLERANCE: usize = 5;

static BACKTICK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"`([^`]+)`").expect("backtick regex"));

static CELL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\d+)\s*/\s*(\d+)").expect("cell regex"));

/// One parsed table row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableRow {
    /// First backtick group in the first cell.
    pub key: String,
    /// Header name without angle brackets, for header rows.
    pub header: Option<String>,
    /// Whether the first cell carries `(var)`.
    pub data: bool,
    /// The nine donor cells as printed by doc 15.
    pub cells: Vec<String>,
    /// Source line number.
    pub line: usize,
}

/// The parsed doc 15 table.
#[derive(Clone, Debug, Default)]
pub struct Table {
    /// Data rows in order.
    pub rows: Vec<TableRow>,
}

impl Table {
    /// Number of symbol rows.
    pub fn symbol_rows(&self) -> usize {
        self.rows.iter().filter(|row| row.header.is_none()).count()
    }

    /// Number of header rows.
    pub fn header_rows(&self) -> usize {
        self.rows.iter().filter(|row| row.header.is_some()).count()
    }
}

/// Parse the `#### 3.B.4` table of doc 15.
pub fn parse_table(path: &Path) -> Result<Table> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse_table_text(&text)
}

/// Parse the table from text, used by the tests.
pub fn parse_table_text(text: &str) -> Result<Table> {
    let mut in_section = false;
    let mut table = Table::default();
    for (index, line) in text.lines().enumerate() {
        if line.starts_with("#### 3.B.4") {
            in_section = true;
            continue;
        }
        if line.starts_with("#### 3.B.5") {
            break;
        }
        if !in_section || !line.starts_with("| ") {
            continue;
        }
        if line
            .chars()
            .all(|character| matches!(character, '|' | '-' | ':' | ' '))
        {
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
        if cells.len() < 10 {
            continue;
        }
        let Some(key) = BACKTICK_RE
            .captures(&cells[0])
            .and_then(|capture| capture.get(1))
            .map(|value| value.as_str().to_string())
        else {
            continue;
        };
        let header = key
            .strip_prefix('<')
            .and_then(|value| value.strip_suffix('>'))
            .map(str::to_string);
        let data = cells[0].contains("(var)");
        let donors: Vec<String> = cells[1..10].to_vec();
        table.rows.push(TableRow {
            key,
            header,
            data,
            cells: donors,
            line: index + 1,
        });
    }
    if table.rows.is_empty() {
        bail!("no data rows between `#### 3.B.4` and `#### 3.B.5`");
    }
    Ok(table)
}

/// One mismatch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Difference {
    /// Table key.
    pub key: String,
    /// Donor column.
    pub donor: String,
    /// Doc 15 text.
    pub doc: String,
    /// Tool value.
    pub tool: String,
    /// Whether the row is a header row.
    pub header: bool,
}

/// Options for `compare-table`.
#[derive(Clone, Debug)]
pub struct CompareOptions {
    /// The doc 15 file.
    pub table: PathBuf,
    /// The resources directory.
    pub resources: PathBuf,
    /// Fail when a donor is missing.
    pub require_all: bool,
    /// `known-differences.toml` path.
    pub known: PathBuf,
    /// Where `compare-table.md` is written.
    pub out: PathBuf,
}

impl Default for CompareOptions {
    fn default() -> Self {
        Self {
            table: PathBuf::new(),
            resources: PathBuf::from("ravenbsd-resources"),
            require_all: false,
            known: PathBuf::from("libc/ravenlibc/abi/census/known-differences.toml"),
            out: PathBuf::from("tools/libc-census/out"),
        }
    }
}

/// The comparison result.
#[derive(Clone, Debug, Default)]
pub struct CompareReport {
    /// Symbol rows in the table.
    pub symbol_rows: usize,
    /// Header rows in the table.
    pub header_rows: usize,
    /// Symbol mismatches.
    pub symbol_differences: Vec<Difference>,
    /// Header mismatches.
    pub header_differences: Vec<Difference>,
    /// Donors that have no root on disk.
    pub missing_donors: Vec<String>,
    /// Donors that were compared.
    pub present_donors: Vec<String>,
    /// A donor was missing.
    pub partial: bool,
    /// The table had a symbol row with no leading `D/H`.
    pub parse_errors: Vec<String>,
    /// Exit code contract.
    pub status: Exit,
}

impl CompareReport {
    /// Whether the run passed.
    pub fn passed(&self) -> bool {
        self.status == Exit::Ok
    }
}

/// Run the comparison and write the report.
pub fn compare(options: &CompareOptions) -> Result<CompareReport> {
    let table = parse_table(&options.table)?;
    let donors: Vec<String> = crate::model::DONORS
        .iter()
        .map(|donor| (*donor).to_string())
        .collect();
    let present: Vec<String> = donors
        .iter()
        .filter(|donor| donor_present(&options.resources, donor))
        .cloned()
        .collect();
    let missing: Vec<String> = donors
        .iter()
        .filter(|donor| !donor_present(&options.resources, donor))
        .cloned()
        .collect();

    let universe = Universe::build(table.rows.iter().map(|row| {
        (
            row.key.clone(),
            if row.header.is_some() {
                crate::model::Kind::HeaderOnly
            } else if row.data {
                crate::model::Kind::Data
            } else {
                crate::model::Kind::Func
            },
        )
    }));
    let scans = scan_donors(
        &options.resources,
        &universe,
        &ScanOptions {
            prefilter: true,
            jobs: 1,
            donors: present.clone(),
        },
    )?;
    let rows = cell_rows(&universe, &scans, &present);
    let by_name: BTreeMap<&str, &crate::model::CellRow> =
        rows.iter().map(|row| (row.name.as_str(), row)).collect();
    let header_paths = header_paths(&options.resources, &present)?;

    let mut report = CompareReport {
        symbol_rows: table.symbol_rows(),
        header_rows: table.header_rows(),
        missing_donors: missing.clone(),
        present_donors: present.clone(),
        partial: !missing.is_empty(),
        ..CompareReport::default()
    };

    for row in &table.rows {
        for (index, donor) in donors.iter().enumerate() {
            if !present.contains(donor) {
                continue;
            }
            let doc = row.cells.get(index).cloned().unwrap_or_default();
            if let Some(header) = &row.header {
                let doc_yes = doc
                    .replace('*', "")
                    .split_whitespace()
                    .next()
                    .is_some_and(|word| word == "yes");
                let tool_yes = header_paths.get(donor).is_some_and(|paths| {
                    paths
                        .iter()
                        .any(|path| path.ends_with(&format!("/{header}")))
                });
                if doc_yes != tool_yes {
                    report.header_differences.push(Difference {
                        key: row.key.clone(),
                        donor: donor.clone(),
                        doc: doc.clone(),
                        tool: if tool_yes { "yes" } else { "no" }.to_string(),
                        header: true,
                    });
                }
            } else {
                let Some((doc_d, doc_h)) = parse_symbol_cell(&doc) else {
                    report
                        .parse_errors
                        .push(format!("{}: donor {donor} cell `{doc}`", row.key));
                    report.symbol_differences.push(Difference {
                        key: row.key.clone(),
                        donor: donor.clone(),
                        doc: doc.clone(),
                        tool: "unparsed".to_string(),
                        header: false,
                    });
                    continue;
                };
                let cell = by_name
                    .get(row.key.as_str())
                    .map(|row| row.cells.get(donor).copied().unwrap_or_default())
                    .unwrap_or_default();
                if cell.d != doc_d || cell.h != doc_h {
                    report.symbol_differences.push(Difference {
                        key: row.key.clone(),
                        donor: donor.clone(),
                        doc: doc.clone(),
                        tool: cell.text(),
                        header: false,
                    });
                }
            }
        }
    }

    let known = load_known(&options.known)?;
    let tolerated = |difference: &Difference| {
        known.difference.iter().any(|known| {
            known.key == difference.key
                && known.donor == difference.donor
                && !known.reason.trim().is_empty()
        })
    };
    let unlisted_headers = report
        .header_differences
        .iter()
        .filter(|difference| !tolerated(difference))
        .count();
    let headers_ok = report.header_differences.len() <= HEADER_TOLERANCE && unlisted_headers == 0;
    let symbols_ok = report.symbol_differences.is_empty();

    report.status = if options.require_all && report.partial {
        Exit::Usage
    } else if symbols_ok && headers_ok {
        Exit::Ok
    } else {
        Exit::GateFailed
    };
    write_report(&options.out.join("compare-table.md"), &report)?;
    Ok(report)
}

fn parse_symbol_cell(cell: &str) -> Option<(u32, u32)> {
    let cleaned = cell.replace('*', "");
    let capture = CELL_RE.captures(&cleaned)?;
    let d = capture.get(1)?.as_str().parse().ok()?;
    let h = capture.get(2)?.as_str().parse().ok()?;
    Some((d, h))
}

fn load_known(path: &Path) -> Result<KnownDifferences> {
    if !path.exists() {
        return Ok(KnownDifferences::default());
    }
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

fn header_paths(resources: &Path, donors: &[String]) -> Result<BTreeMap<String, BTreeSet<String>>> {
    let mut output = BTreeMap::new();
    for donor in donors {
        let mut paths = BTreeSet::new();
        for root in donor_roots(donor) {
            let path = resources.join(root);
            if path.is_dir() {
                walk_headers(&path, resources, &mut paths)?;
            }
        }
        output.insert(donor.clone(), paths);
    }
    Ok(output)
}

fn walk_headers(directory: &Path, resources: &Path, paths: &mut BTreeSet<String>) -> Result<()> {
    let mut entries = Vec::new();
    for entry in
        fs::read_dir(directory).with_context(|| format!("reading {}", directory.display()))?
    {
        entries.push(entry?);
    }
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let relative = path.strip_prefix(resources).unwrap_or(&path);
        if relative.components().any(|component| {
            let text = component.as_os_str().to_string_lossy();
            matches!(text.as_ref(), "tests" | "test" | "regress" | "testsuite")
                || text.contains("test")
        }) {
            continue;
        }
        if entry.file_type()?.is_dir() {
            walk_headers(&path, resources, paths)?;
        } else if matches!(
            path.extension().and_then(|value| value.to_str()),
            Some("h" | "hpp")
        ) {
            paths.insert(relative.to_string_lossy().to_string());
        }
    }
    Ok(())
}

fn write_report(path: &Path, report: &CompareReport) -> Result<()> {
    let mut text = String::new();
    text.push_str("<!-- SPDX-License-Identifier: BSD-3-Clause -->\n");
    text.push_str("<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->\n");
    text.push_str("<!-- GENERATED by libc-census; do not edit -->\n\n");
    text.push_str("# Doc 15 census comparison\n\n");
    text.push_str(&format!(
        "Symbol rows: {}, header rows: {}.\n\n",
        report.symbol_rows, report.header_rows
    ));
    text.push_str(&format!(
        "Present donors: {}.\n",
        report.present_donors.join(", ")
    ));
    text.push_str(&format!(
        "Missing donors: {}.\n\n",
        if report.missing_donors.is_empty() {
            "none".to_string()
        } else {
            report.missing_donors.join(", ")
        }
    ));
    text.push_str(&format!(
        "Symbol differences: {} (gate: 0).\n",
        report.symbol_differences.len()
    ));
    text.push_str(&format!(
        "Header differences: {} (gate: at most {} and listed).\n\n",
        report.header_differences.len(),
        HEADER_TOLERANCE
    ));
    let differences: Vec<&Difference> = report
        .symbol_differences
        .iter()
        .chain(report.header_differences.iter())
        .collect();
    if differences.is_empty() {
        text.push_str("_No differences._\n");
    } else {
        text.push_str("| kind | key | donor | doc | tool |\n|---|---|---|---|---|\n");
        for difference in differences {
            text.push_str(&format!(
                "| {} | `{}` | {} | {} | {} |\n",
                if difference.header {
                    "header"
                } else {
                    "symbol"
                },
                difference.key,
                difference.donor,
                difference.doc,
                difference.tool
            ));
        }
    }
    text.push_str(&format!(
        "\nResult: {}.\n",
        if report.passed() {
            "PASS"
        } else if report.status == Exit::Usage {
            "PARTIAL (--require-all)"
        } else {
            "FAIL"
        }
    ));
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
#[path = "table15_ut.rs"]
mod tests;

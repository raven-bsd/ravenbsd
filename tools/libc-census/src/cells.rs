// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The donor cell scan: roots, recursive walk, regexes and prefilter.
//!
//! The cell definition is doc 15 3.B.3 reproduced exactly; the regex text is
//! ticket C33 section 4.3. The scan never interprets donor code, it only counts
//! matching file names. `illumos` files are inputs like any other donor; the
//! tool does not print their contents.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use anyhow::{Context, Result};
use regex::bytes::Regex;

use crate::model::{Cell, CellRow, DONORS, N6_DONORS, donor_family};
use crate::universe::Universe;

/// Maximum file size read by the scan.
pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

/// How many leading bytes are checked for a NUL byte.
pub const BINARY_PREFIX_BYTES: usize = 8 * 1024;

/// Source extensions that enter the walk.
const SOURCE_EXTENSIONS: [&str; 6] = ["c", "cc", "cpp", "S", "h", "hpp"];

/// Path components that are never scanned.
const EXCLUDED_COMPONENTS: [&str; 4] = ["tests", "test", "regress", "testsuite"];

/// Roots per donor, relative to the resources directory (ticket 4.3).
pub fn donor_roots(donor: &str) -> &'static [&'static str] {
    match donor {
        "glibc" => &["glibc"],
        "musl" => &["musl/src", "musl/include"],
        "freebsd" => &[
            "freebsd/contrib/libc-vis",
            "freebsd/contrib/libc-pwcache",
            "freebsd/contrib/libexecinfo",
            "freebsd/lib/libuuid",
            "freebsd/lib/libc",
            "freebsd/lib/libutil",
            "freebsd/lib/libsys",
            "freebsd/include",
            "freebsd/sys/sys",
        ],
        "netbsd" => &[
            "netbsd/common/lib/libc",
            "netbsd/lib/libc",
            "netbsd/lib/libutil",
            "netbsd/include",
            "netbsd/sys/sys",
        ],
        "openbsd" => &[
            "openbsd/lib/libc",
            "openbsd/lib/libutil",
            "openbsd/include",
            "openbsd/sys/sys",
        ],
        "bionic" => &["bionic/libc"],
        "illumos" => &[
            "illumos-gate/usr/src/lib/libc",
            "illumos-gate/usr/src/head",
            "illumos-gate/usr/src/uts/common/sys",
        ],
        "mlibc" => &["mlibc/options", "mlibc/sysdeps"],
        "picolibc" => &["picolibc/libc"],
        _ => &[],
    }
}

/// Options for one scan.
#[derive(Clone, Debug)]
pub struct ScanOptions {
    /// Run the mandatory identifier prefilter.
    pub prefilter: bool,
    /// Number of worker threads.
    pub jobs: usize,
    /// Donors to scan, in column order.
    pub donors: Vec<String>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            prefilter: true,
            jobs: 1,
            donors: DONORS.iter().map(|donor| (*donor).to_string()).collect(),
        }
    }
}

/// What one donor scan produced.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DonorScan {
    /// Donor name.
    pub donor: String,
    /// Per-name cells, sorted.
    pub cells: BTreeMap<String, Cell>,
    /// Selected source files (before binary/large skips).
    pub files: usize,
    /// Files with a NUL byte in the first 8 KiB.
    pub skipped_binary: usize,
    /// Files over 16 MiB.
    pub skipped_large: usize,
    /// Roots that do not exist, relative to the resources directory.
    pub missing_roots: Vec<String>,
}

/// Whether a donor has at least one root on disk.
pub fn donor_present(resources: &Path, donor: &str) -> bool {
    donor_roots(donor)
        .iter()
        .any(|root| resources.join(root).is_dir())
}

/// The regex forms, exposed for the unit tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    /// A function definition or declaration.
    Definition,
    /// A `#define`.
    Define,
    /// A variable declaration.
    Variable,
    /// A `__ssp_real(NAME)` redirection.
    SspReal,
    /// An alias macro.
    Alias,
}

static IDENT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\x41-\x5aa-z_][\x41-\x5aa-z0-9_]*").expect("identifier regex"));

fn pattern(form: Form, name: &str) -> String {
    let name = regex::escape(name);
    match form {
        Form::Definition => format!(
            r"(?m)__ssp_real\({name}\)|^(?:[\x41-\x5aa-z_][^;=#\n]*[ *\t])?(?:__)?{name}[ \t]*\(|^#[ \t]*define[ \t]+{name}\b|(?:weak_alias|strong_alias|__weak_alias|__strong_alias|DEF_WEAK|DEF_STRONG|versioned_symbol|libc_hidden_def)[ \t]*\([^\n]*\b{name}\b"
        ),
        Form::Define => format!(r"(?m)^#[ \t]*define[ \t]+{name}\b"),
        Form::Variable => format!(
            r"(?m)__ssp_real\({name}\)|^(?:[\x41-\x5aa-z_][^;=#(\n]*[ *\t]){name}[ \t]*(?:\[|;|=)|^#[ \t]*define[ \t]+{name}\b"
        ),
        Form::SspReal => format!(r"__ssp_real\({name}\)"),
        Form::Alias => format!(
            r"(?:weak_alias|strong_alias|__weak_alias|__strong_alias|DEF_WEAK|DEF_STRONG|versioned_symbol|libc_hidden_def)[ \t]*\([^\n]*\b{name}\b"
        ),
    }
}

fn compile(form: Form, name: &str) -> Regex {
    Regex::new(&pattern(form, name)).expect("census regex")
}

/// Whether `form` matches `line` for `name`; used by the form tests.
pub fn form_matches(form: Form, line: &str, name: &str) -> bool {
    compile(form, name).is_match(line.as_bytes())
}

struct Matchers {
    entries: Vec<(String, Regex)>,
    index: HashMap<String, usize>,
}

impl Matchers {
    fn new(universe: &Universe) -> Self {
        let mut entries = Vec::with_capacity(universe.entries.len());
        let mut index = HashMap::with_capacity(universe.entries.len());
        for entry in &universe.entries {
            let form = if entry.kind.is_data() {
                Form::Variable
            } else {
                Form::Definition
            };
            index.insert(entry.name.clone(), entries.len());
            entries.push((entry.name.clone(), compile(form, &entry.name)));
        }
        Self { entries, index }
    }

    fn candidates(&self, bytes: &[u8], prefilter: bool) -> HashSet<usize> {
        if !prefilter {
            return (0..self.entries.len()).collect();
        }
        let mut candidates = HashSet::new();
        for token in IDENT_RE.find_iter(bytes) {
            let Ok(text) = std::str::from_utf8(token.as_bytes()) else {
                continue;
            };
            if let Some(index) = self.index.get(text) {
                candidates.insert(*index);
            }
            if let Some(stripped) = text.strip_prefix("__")
                && let Some(index) = self.index.get(stripped)
            {
                candidates.insert(*index);
            }
        }
        candidates
    }
}

/// Scan the selected donors and return their cells.
pub fn scan_donors(
    resources: &Path,
    universe: &Universe,
    options: &ScanOptions,
) -> Result<BTreeMap<String, DonorScan>> {
    let matchers = Matchers::new(universe);
    let jobs = options.jobs.max(1).min(options.donors.len().max(1));
    let mut results = BTreeMap::new();
    std::thread::scope(|scope| -> Result<()> {
        let mut handles = Vec::new();
        for worker in 0..jobs {
            let donors: Vec<String> = options
                .donors
                .iter()
                .skip(worker)
                .step_by(jobs)
                .cloned()
                .collect();
            let matchers = &matchers;
            handles.push(scope.spawn(move || -> Result<Vec<DonorScan>> {
                let mut scans = Vec::new();
                for donor in donors {
                    scans.push(scan_donor(resources, &donor, matchers, options)?);
                }
                Ok(scans)
            }));
        }
        for handle in handles {
            let scans = handle
                .join()
                .map_err(|_| anyhow::anyhow!("a scan worker panicked"))??;
            for scan in scans {
                results.insert(scan.donor.clone(), scan);
            }
        }
        Ok(())
    })?;
    Ok(results)
}

/// Build `cells.json`-shaped rows from donor scans.
pub fn cell_rows(
    universe: &Universe,
    scans: &BTreeMap<String, DonorScan>,
    donors: &[String],
) -> Vec<CellRow> {
    let mut rows = Vec::new();
    for entry in &universe.entries {
        let mut cells = BTreeMap::new();
        let mut libcs = Vec::new();
        let mut n6 = 0u32;
        let mut families = BTreeSet::new();
        for donor in donors {
            let cell = scans
                .get(donor)
                .and_then(|scan| scan.cells.get(&entry.name).copied())
                .unwrap_or_default();
            if cell.total() > 0 {
                libcs.push(donor.clone());
                if N6_DONORS.contains(&donor.as_str()) {
                    n6 = n6.saturating_add(1);
                }
                if let Some(family) = donor_family(donor) {
                    families.insert(family);
                }
            }
            cells.insert(donor.clone(), cell);
        }
        libcs.sort();
        rows.push(CellRow {
            name: entry.name.clone(),
            kind: entry.kind,
            cells,
            n6,
            libcs,
            families: families.into_iter().map(str::to_string).collect(),
        });
    }
    rows
}

fn scan_donor(
    resources: &Path,
    donor: &str,
    matchers: &Matchers,
    options: &ScanOptions,
) -> Result<DonorScan> {
    let mut scan = DonorScan {
        donor: donor.to_string(),
        ..DonorScan::default()
    };
    let mut files = Vec::new();
    for root in donor_roots(donor) {
        let path = resources.join(root);
        if !path.is_dir() {
            scan.missing_roots.push((*root).to_string());
            continue;
        }
        walk(&path, resources, &mut files)?;
    }
    files.sort();
    scan.files = files.len();

    for file in files {
        let metadata = fs::metadata(&file).with_context(|| format!("stat {}", file.display()))?;
        if metadata.len() > MAX_FILE_BYTES {
            scan.skipped_large += 1;
            continue;
        }
        let bytes = fs::read(&file).with_context(|| format!("read {}", file.display()))?;
        let prefix = &bytes[..bytes.len().min(BINARY_PREFIX_BYTES)];
        if prefix.contains(&0) {
            scan.skipped_binary += 1;
            continue;
        }
        let header = is_header(&file);
        for index in matchers.candidates(&bytes, options.prefilter) {
            let (name, regex) = &matchers.entries[index];
            if !regex.is_match(&bytes) {
                continue;
            }
            let cell = scan.cells.entry(name.clone()).or_default();
            if header {
                cell.h = cell.h.saturating_add(1);
            } else {
                cell.d = cell.d.saturating_add(1);
            }
        }
    }
    Ok(scan)
}

fn walk(directory: &Path, resources: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries = Vec::new();
    for entry in
        fs::read_dir(directory).with_context(|| format!("read_dir {}", directory.display()))?
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
            EXCLUDED_COMPONENTS.contains(&text.as_ref()) || text.contains("test")
        }) {
            continue;
        }
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            walk(&path, resources, files)?;
        } else if file_type.is_file() {
            let extension = path.extension().and_then(|value| value.to_str());
            if extension.is_some_and(|value| SOURCE_EXTENSIONS.contains(&value)) {
                files.push(path);
            }
        }
    }
    Ok(())
}

fn is_header(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| matches!(value, "h" | "hp" | "hpp"))
}

#[cfg(test)]
#[path = "cells_ut.rs"]
mod tests;

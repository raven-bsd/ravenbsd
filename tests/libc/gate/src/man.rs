// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Man-page candidate selection and coverage checking.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// The fixed M4 deviation list from `C74-libc-gate-report.md` 4.6.
pub const M4: &[&str] = &[
    "setlocale",
    "newlocale",
    "uselocale",
    "duplocale",
    "nl_langinfo",
    "localeconv",
    "printf",
    "fprintf",
    "sprintf",
    "snprintf",
    "dprintf",
    "vprintf",
    "iconv",
    "iconv_open",
    "iconv_close",
    "assert",
    "__assert_fail",
    "fork",
    "vfork",
    "posix_spawn",
    "posix_spawnp",
    "system",
    "popen",
    "issetugid",
    "getauxval",
    "arc4random",
    "arc4random_buf",
    "arc4random_uniform",
    "getentropy",
    "getrandom",
    "malloc",
    "free",
    "calloc",
    "realloc",
    "posix_memalign",
    "aligned_alloc",
    "clock",
    "times",
    "clock_settime",
    "clock_getcpuclockid",
    "adjtime",
    "mmap",
    "munmap",
    "mprotect",
    "madvise",
    "mincore",
    "open",
    "openat",
    "fcntl",
    "ioctl",
    "pledge",
    "unveil",
    "pthread_create",
    "pthread_cancel",
    "pthread_mutex_lock",
    "dlopen",
    "dlsym",
    "dlerror",
    "dl_iterate_phdr",
    "sysconf",
    "ARG_MAX",
    "NGROUPS_MAX",
    "HOST_NAME_MAX",
];

/// One census symbol.
#[derive(Clone, Debug, Deserialize)]
pub struct Symbol {
    /// Name.
    pub name: String,
    /// Ring; an integer or the string `private`.
    #[serde(default)]
    pub ring: Option<toml::Value>,
    /// Platform gaps.
    #[serde(default)]
    pub gap: Vec<String>,
    /// Deprecation/link warning.
    #[serde(default)]
    pub warn: Option<toml::Value>,
    /// Stub status.
    #[serde(default)]
    pub stub: Option<toml::Value>,
    /// Area number.
    #[serde(default)]
    pub area: Option<u8>,
    /// Standard tags.
    #[serde(default)]
    pub standard: Vec<String>,
    /// Personality tags.
    #[serde(default)]
    pub families: Vec<String>,
    /// Personality.
    #[serde(default)]
    pub personality: Option<String>,
    /// Header.
    #[serde(default)]
    pub header: Option<String>,
}

#[derive(Deserialize)]
struct SymbolsFile {
    #[serde(default)]
    symbol: Vec<Symbol>,
}

/// A selected man-page candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    /// Symbol name.
    pub name: String,
    /// Ring.
    pub ring: u8,
    /// Rules that selected it.
    pub rules: Vec<&'static str>,
}

/// One `man-pages.toml` page.
#[derive(Clone, Debug, Deserialize)]
pub struct PageSpec {
    /// Page base name (without section).
    pub name: String,
    /// Section, default 3.
    #[serde(default = "default_section")]
    pub section: u8,
    /// Names covered by the page.
    #[serde(default)]
    pub names: Vec<String>,
    /// Selection reasons (`M1`..`M4`, `extra`).
    #[serde(default)]
    pub reasons: Vec<String>,
    /// Area numbers.
    #[serde(default)]
    pub areas: Vec<u8>,
    /// Source documents.
    #[serde(default)]
    pub sources: Vec<String>,
}

const fn default_section() -> u8 {
    3
}

#[derive(Deserialize)]
struct PagesFile {
    #[serde(default)]
    page: Vec<PageSpec>,
}

/// The result of `man-check`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ManReport {
    /// Candidate names.
    pub candidates: Vec<String>,
    /// Number of pages.
    pub pages: usize,
    /// Number of covered names.
    pub covered: usize,
    /// Errors; empty means the check passed.
    pub errors: Vec<String>,
}

impl ManReport {
    /// True when there is no error.
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Read the census symbols.
pub fn load_symbols(path: &Path) -> Result<Vec<Symbol>> {
    let text =
        fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    let file: SymbolsFile =
        toml::from_str(&text).with_context(|| format!("cannot parse {}", path.display()))?;
    Ok(file.symbol)
}

fn value_non_empty(value: &toml::Value) -> bool {
    match value {
        toml::Value::String(text) => !text.is_empty(),
        toml::Value::Array(items) => !items.is_empty(),
        toml::Value::Boolean(flag) => *flag,
        _ => false,
    }
}

fn warn_matches(value: &toml::Value) -> bool {
    match value {
        toml::Value::String(text) => matches!(text.as_str(), "deprecated" | "link-time"),
        toml::Value::Array(items) => items.iter().any(|item| {
            item.as_str()
                .is_some_and(|text| matches!(text, "deprecated" | "link-time"))
        }),
        _ => false,
    }
}

/// Select the candidates of 4.6. The M2/M3 rules naturally produce nothing when the census has no
/// `gap`/`warn`/`stub` fields; the M1 and M4 lists are the fallback.
#[must_use]
pub fn candidates(symbols: &[Symbol]) -> Vec<Candidate> {
    let mut selected = Vec::new();
    for symbol in symbols {
        let Some(ring) = symbol
            .ring
            .as_ref()
            .and_then(toml::Value::as_integer)
            .and_then(|value| u8::try_from(value).ok())
        else {
            continue;
        };
        if ring > 2 {
            continue;
        }
        let mut rules = Vec::new();
        if symbol.name.starts_with("raven_") {
            rules.push("M1");
        }
        if !symbol.gap.is_empty() {
            rules.push("M2");
        }
        if symbol.warn.as_ref().is_some_and(warn_matches)
            || symbol.stub.as_ref().is_some_and(value_non_empty)
        {
            rules.push("M3");
        }
        if M4.contains(&symbol.name.as_str()) {
            rules.push("M4");
        }
        if !rules.is_empty() {
            selected.push(Candidate {
                name: symbol.name.clone(),
                ring,
                rules,
            });
        }
    }
    selected.sort_by(|left, right| left.name.cmp(&right.name));
    selected.dedup_by(|left, right| left.name == right.name);
    selected
}

/// Render the candidate list deterministically.
#[must_use]
pub fn list_candidates(symbols: &[Symbol]) -> String {
    let mut out = String::new();
    for candidate in candidates(symbols) {
        out.push_str(&format!(
            "{}\t{}\t{}\n",
            candidate.name,
            candidate.rules.join(","),
            candidate.ring
        ));
    }
    out
}

/// Load `man-pages.toml`.
///
/// # Errors
///
/// Returns an error for an unreadable or malformed file.
pub fn load_pages(path: &Path) -> Result<Vec<PageSpec>> {
    let text =
        fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    let file: PagesFile =
        toml::from_str(&text).with_context(|| format!("cannot parse {}", path.display()))?;
    Ok(file.page)
}

/// Required `.Sh` sections by section number.
fn required_sections(section: u8) -> &'static [&'static str] {
    if section == 7 {
        &[
            "NAME",
            "SYNOPSIS",
            "DESCRIPTION",
            "FILES",
            "ENVIRONMENT",
            "STANDARDS",
            "SEE ALSO",
        ]
    } else {
        &[
            "NAME",
            "SYNOPSIS",
            "DESCRIPTION",
            "RETURN VALUES",
            "ERRORS",
            "STANDARDS",
            "CAVEATS",
            "SEE ALSO",
        ]
    }
}

fn page_path(docs: &Path, page: &PageSpec) -> PathBuf {
    docs.join(format!("man{}", page.section))
        .join(format!("{}.{}", page.name, page.section))
}

fn has_section(text: &str, section: &str) -> bool {
    text.lines()
        .any(|line| line.trim() == format!(".Sh {section}"))
}

fn has_nm(text: &str, name: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with(".Nm") && line.split_whitespace().any(|word| word.trim() == name)
    })
}

/// Check page coverage, required sections and orphans.
///
/// # Errors
///
/// Returns an error for unreadable inputs; content failures are in [`ManReport::errors`].
pub fn check(
    _repo: &Path,
    symbols_path: &Path,
    pages_path: &Path,
    docs: &Path,
) -> Result<ManReport> {
    let symbols = load_symbols(symbols_path)?;
    let pages = load_pages(pages_path)?;
    let candidates = candidates(&symbols);
    let candidate_names: BTreeSet<&str> = candidates
        .iter()
        .map(|candidate| candidate.name.as_str())
        .collect();
    let mut report = ManReport {
        candidates: candidates
            .iter()
            .map(|candidate| candidate.name.clone())
            .collect(),
        pages: pages.len(),
        ..ManReport::default()
    };
    let mut covered = BTreeSet::new();
    for page in &pages {
        let path = page_path(docs, page);
        if !path.is_file() {
            report
                .errors
                .push(format!("missing page {}", path.display()));
            continue;
        }
        let text =
            fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
        for section in required_sections(page.section) {
            if !has_section(&text, section) {
                report
                    .errors
                    .push(format!("{}: missing .Sh {section}", path.display()));
            }
        }
        for name in &page.names {
            if !has_nm(&text, name) {
                report
                    .errors
                    .push(format!("{}: no .Nm {name}", path.display()));
            }
            if page.section == 3
                && !candidate_names.contains(name.as_str())
                && !page.reasons.iter().any(|reason| reason == "extra")
            {
                report
                    .errors
                    .push(format!("{}: `{name}` is not a candidate", path.display()));
            }
            covered.insert(name.clone());
        }
    }
    for candidate in &candidates {
        if !covered.contains(&candidate.name) {
            report
                .errors
                .push(format!("candidate `{}` has no page", candidate.name));
        }
    }
    report.covered = covered.len();
    report.errors.sort();
    report.errors.dedup();
    check_orphans(docs, &pages, &mut report)?;
    Ok(report)
}

fn check_orphans(docs: &Path, pages: &[PageSpec], report: &mut ManReport) -> Result<()> {
    let listed: BTreeSet<String> = pages
        .iter()
        .map(|page| page_path(docs, page).to_string_lossy().to_string())
        .collect();
    for (section, root) in [(3u8, docs.join("man3")), (7u8, docs.join("man7"))] {
        if !root.is_dir() {
            continue;
        }
        for entry in
            fs::read_dir(&root).with_context(|| format!("cannot list {}", root.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            let suffix = format!(".{section}");
            if path.extension().and_then(|value| value.to_str()) != Some(&suffix[1..]) {
                continue;
            }
            if !listed.contains(&path.to_string_lossy().to_string()) {
                report
                    .errors
                    .push(format!("orphan page {}", path.display()));
            }
        }
    }
    Ok(())
}

/// Convenience wrapper that loads the default paths.
///
/// # Errors
///
/// Returns an error for unreadable inputs.
pub fn check_default(repo: &Path) -> Result<ManReport> {
    let symbols = repo.join("libc/ravenlibc/abi/census/symbols.toml");
    let pages = repo.join("tests/libc/gate/man-pages.toml");
    let docs = repo.join("docs");
    check(repo, &symbols, &pages, &docs)
}

/// Ensure a page list is not empty and every page has names.
///
/// # Errors
///
/// Returns an error for an empty manifest or a page with no names.
pub fn validate_pages(pages: &[PageSpec]) -> Result<()> {
    if pages.is_empty() {
        bail!("man-pages.toml has no pages");
    }
    for page in pages {
        if page.name.is_empty() || page.names.is_empty() {
            bail!("page `{}` has no names", page.name);
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "man_ut.rs"]
mod tests;

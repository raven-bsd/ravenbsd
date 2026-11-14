// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Failure classification from compiler, linker, configure and CMake logs (section 4.7).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Deserialize;

/// One step log for the classifier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StepLog {
    /// Step name (`configure`, `make`, `install`, ...).
    pub name: String,
    /// Exit code (`None` for a signal or timeout).
    pub exit: Option<i32>,
    /// Combined text.
    pub text: String,
}

/// The names the classifier may use from the C33 census.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SymbolIndex {
    /// Headers of symbols with ring <= 2.
    pub headers: BTreeSet<String>,
    /// Symbol names with ring <= 2.
    pub symbols: BTreeSet<String>,
    /// Ring of each symbol with ring <= 2.
    pub rings: BTreeMap<String, u32>,
    /// The table was loaded; without it the probe rules are skipped.
    pub complete: bool,
}

#[derive(Deserialize)]
struct Census {
    #[serde(default)]
    symbol: Vec<CensusSymbol>,
}

#[derive(Deserialize)]
struct CensusSymbol {
    name: String,
    #[serde(default)]
    ring: Option<u32>,
    #[serde(default)]
    header: Option<String>,
}

impl SymbolIndex {
    /// Load `symbols.toml`; a missing file yields an incomplete empty index.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.is_file() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(path)?;
        let census: Census = toml::from_str(&text)?;
        let mut index = Self {
            complete: true,
            ..Self::default()
        };
        for symbol in census.symbol {
            if symbol.ring.is_some_and(|ring| ring <= 2) {
                let ring = symbol.ring.unwrap_or(0);
                index.rings.insert(symbol.name.clone(), ring);
                index.symbols.insert(symbol.name);
                if let Some(header) = symbol.header.filter(|header| !header.is_empty()) {
                    index.headers.insert(header);
                }
            }
        }
        Ok(index)
    }

    /// An index built in a test.
    #[must_use]
    pub fn from_names(symbols: &[&str], headers: &[&str]) -> Self {
        Self {
            headers: headers.iter().map(|value| (*value).to_string()).collect(),
            symbols: symbols.iter().map(|value| (*value).to_string()).collect(),
            rings: symbols
                .iter()
                .map(|value| ((*value).to_string(), 1))
                .collect(),
            complete: true,
        }
    }
}

/// The result of classifying a failed package.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Classified {
    /// The first matching class by rule order.
    pub class: String,
    /// Its detail.
    pub detail: String,
    /// Missing headers.
    pub missing_headers: BTreeSet<String>,
    /// Missing symbols (link and implicit).
    pub missing_symbols: BTreeSet<String>,
    /// Implicit function declarations.
    pub implicit_decls: BTreeSet<String>,
    /// Missing libraries.
    pub missing_libs: BTreeSet<String>,
    /// Configure probes of claimed libc symbols that answered `no`.
    pub libc_probe_failed: BTreeSet<String>,
    /// Every class that matched and how many times.
    pub findings: BTreeMap<String, u32>,
    /// First error-looking line of the first failing step.
    pub first_error: Option<String>,
}

/// Classify the logs of one package; `None` when no step failed and no rule matched.
#[must_use]
pub fn classify(
    logs: &[StepLog],
    index: &SymbolIndex,
    replacements: &[(PathBuf, &'static str)],
) -> Option<Classified> {
    let mut missing_headers = BTreeSet::new();
    let mut missing_symbols = BTreeSet::new();
    let mut implicit_decls = BTreeSet::new();
    let mut missing_libs = BTreeSet::new();
    let mut libc_probe_failed = BTreeSet::new();
    let mut configure_error: Option<String> = None;
    let mut platform_detail: Option<String> = None;
    let mut other_detail: Option<String> = None;
    let mut findings: BTreeMap<String, u32> = BTreeMap::new();

    for log in logs {
        for raw in log.text.lines() {
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(header) = header_from_fatal(line) {
                missing_headers.insert(header);
                continue;
            }
            if let Some(symbol) = between(line, "call to undeclared function '", "'") {
                implicit_decls.insert(symbol.clone());
                missing_symbols.insert(symbol);
                continue;
            }
            if let Some(symbol) = between(line, "implicit declaration of function '", "'") {
                implicit_decls.insert(symbol.clone());
                missing_symbols.insert(symbol);
                continue;
            }
            if let Some(symbol) = between(line, "implicit declaration of function \"", "\"") {
                implicit_decls.insert(symbol.clone());
                missing_symbols.insert(symbol);
                continue;
            }
            if let Some(symbol) = after(line, "undefined symbol:") {
                missing_symbols.insert(first_token(&symbol));
                continue;
            }
            if let Some(symbol) = between(line, "undefined reference to '", "'") {
                missing_symbols.insert(symbol);
                continue;
            }
            if let Some(symbol) = between(line, "undefined reference to `", "'") {
                missing_symbols.insert(symbol);
                continue;
            }
            if let Some(library) = after(line, "unable to find library -l") {
                missing_libs.insert(first_token(&library));
                continue;
            }
            if let Some(library) = after(line, "cannot find -l") {
                missing_libs.insert(first_token(&library));
                continue;
            }
            if let Some(probe) = probe_name(line) {
                if index.complete {
                    if index.headers.contains(&probe) {
                        missing_headers.insert(probe);
                    } else if index.symbols.contains(&probe) {
                        libc_probe_failed.insert(probe.clone());
                        missing_symbols.insert(probe);
                    }
                }
                continue;
            }
            if let Some(detail) = after(line, "configure: error:") {
                if configure_error.is_none() {
                    configure_error = Some(detail.trim().to_string());
                }
                continue;
            }
            if line.contains("Invalid configuration") {
                platform_detail
                    .get_or_insert_with(|| "config.sub does not know the triple".to_string());
                continue;
            }
            if line.contains("unsupported system") {
                platform_detail.get_or_insert_with(|| line.to_string());
                continue;
            }
            if line.contains("cmake: could not find") {
                platform_detail.get_or_insert_with(|| line.to_string());
                continue;
            }
            if other_detail.is_none() && looks_like_error(line) {
                other_detail = Some(line.to_string());
            }
        }
    }

    let configure_failed = logs
        .iter()
        .any(|log| log.name == "configure" && !matches!(log.exit, Some(0)));

    if !missing_headers.is_empty() {
        findings.insert(
            "missing-header".to_string(),
            u32::try_from(missing_headers.len()).unwrap_or(u32::MAX),
        );
    }
    if !missing_symbols.is_empty() {
        findings.insert(
            "missing-symbol".to_string(),
            u32::try_from(missing_symbols.len()).unwrap_or(u32::MAX),
        );
    }
    if !missing_libs.is_empty() {
        findings.insert(
            "missing-lib".to_string(),
            u32::try_from(missing_libs.len()).unwrap_or(u32::MAX),
        );
    }
    if configure_error.is_some() || configure_failed {
        findings.insert("configure".to_string(), 1);
    }
    if platform_detail.is_some() {
        findings.insert("platform".to_string(), 1);
    }
    if other_detail.is_some() {
        findings.insert("other".to_string(), 1);
    }

    let class = if !missing_headers.is_empty() {
        "missing-header"
    } else if !missing_symbols.is_empty() {
        "missing-symbol"
    } else if !missing_libs.is_empty() {
        "missing-lib"
    } else if configure_error.is_some() || configure_failed {
        "configure"
    } else if platform_detail.is_some() {
        "platform"
    } else if other_detail.is_some() || logs.iter().any(|log| !matches!(log.exit, Some(0))) {
        "other"
    } else {
        return None;
    };

    let detail = match class {
        "missing-header" => missing_headers.iter().next().cloned().unwrap_or_default(),
        "missing-symbol" => missing_symbols.iter().next().cloned().unwrap_or_default(),
        "missing-lib" => missing_libs.iter().next().cloned().unwrap_or_default(),
        "configure" => configure_error
            .clone()
            .or_else(|| {
                logs.iter()
                    .find(|log| log.name == "configure" && !matches!(log.exit, Some(0)))
                    .map(|log| format!("configure exited {:?}", log.exit))
            })
            .unwrap_or_default(),
        "platform" => platform_detail.clone().unwrap_or_default(),
        _ => other_detail.clone().unwrap_or_default(),
    };

    let first_error = logs
        .iter()
        .filter(|log| !matches!(log.exit, Some(0)))
        .find_map(|log| first_error_line(&log.text))
        .map(|line| scrub(&line, replacements));

    Some(Classified {
        class: class.to_string(),
        detail,
        missing_headers,
        missing_symbols,
        implicit_decls,
        missing_libs,
        libc_probe_failed,
        findings,
        first_error,
    })
}

fn header_from_fatal(line: &str) -> Option<String> {
    extract_between(line, "fatal error: '", "' file not found")
        .or_else(|| extract_between(line, "fatal error: \"", "\" file not found"))
        .or_else(|| extract_between(line, "fatal error: ", ": No such file or directory"))
        .filter(|header| !header.is_empty())
}

fn extract_between(line: &str, start: &str, end: &str) -> Option<String> {
    let (_, rest) = line.split_once(start)?;
    let (value, _) = rest.split_once(end)?;
    Some(value.to_string())
}

fn probe_name(line: &str) -> Option<String> {
    let rest = line.strip_prefix("checking for ")?;
    let (name, answer) = rest.split_once("... ")?;
    if answer.trim() != "no" {
        return None;
    }
    Some(name.trim().to_string())
}

fn between(line: &str, prefix: &str, suffix: &str) -> Option<String> {
    let rest = line.split_once(prefix)?.1;
    let value = rest.split_once(suffix)?.0;
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn after(line: &str, prefix: &str) -> Option<String> {
    let rest = line.split_once(prefix)?.1;
    if rest.trim().is_empty() {
        None
    } else {
        Some(rest.trim().to_string())
    }
}

fn first_token(text: &str) -> String {
    text.split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_matches(['"', '\'', '`'])
        .trim_end_matches(':')
        .to_string()
}

fn looks_like_error(line: &str) -> bool {
    line.contains("error:") || line.contains("Error ") || line.starts_with("*** ")
}

fn first_error_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && looks_like_error(line))
        .map(|line| truncate(line, 200))
}

fn truncate(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_string();
    }
    let mut end = limit;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &text[..end])
}

fn scrub(text: &str, replacements: &[(PathBuf, &'static str)]) -> String {
    let mut result = text.to_string();
    for (path, marker) in replacements {
        result = result.replace(&path.display().to_string(), marker);
    }
    truncate(&result, 200)
}

#[cfg(test)]
#[path = "classify_ut.rs"]
mod tests;

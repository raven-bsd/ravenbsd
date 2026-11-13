// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S1/S3 source scans and the S2 undefined-symbol lists (ticket 4.6).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use anyhow::{Context, Result};
use regex::Regex;

use crate::model::{LibraryUse, UndefOutput, UsesOutput};
use crate::universe::Universe;

/// Libraries that ravenlibc subsumes and ships as stubs (ticket 4.6).
pub const STUB_LIBRARIES: [&str; 14] = [
    "m", "pthread", "dl", "rt", "util", "resolv", "crypt", "nsl", "intl", "iconv", "anl", "socket",
    "bsd", "execinfo",
];

/// Code extensions scanned for includes and call sites.
const CODE_EXTENSIONS: [&str; 6] = ["c", "cc", "cpp", "cxx", "h", "hpp"];

static INCLUDE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^\s*#\s*include\s*<([^>]+)>").expect("include regex"));

static CALL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[\x41-\x5aa-z_][\x41-\x5aa-z0-9_]*[ \t]*\(").expect("call regex")
});

static NAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\x41-\x5aa-z_][\x41-\x5aa-z0-9_]*").expect("name regex"));

static DASH_L_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"-l([\x41-\x5aa-z0-9_+.-]+)").expect("-l regex"));

static CHECK_LIB_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"AC_CHECK_LIB\s*\(\s*\[([\x41-\x5aa-z0-9_+.-]+)\]").expect("AC_CHECK_LIB regex")
});

static SEARCH_LIBS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"AC_SEARCH_LIBS\s*\(\s*[^,]+,\s*\[([^\]]*)\]").expect("AC_SEARCH_LIBS regex")
});

/// Scan an unpacked sources tree (S1 and the textual part of S3).
pub fn scan_sources(root: &Path, universe: &Universe) -> Result<UsesOutput> {
    if !root.is_dir() {
        return Ok(UsesOutput {
            skipped: Some("skipped: no sources tree".to_string()),
            ..UsesOutput::default()
        });
    }
    let names: BTreeSet<&str> = universe
        .entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    let mut header_packages: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut library_packages: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut symbol_packages: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut packages = BTreeSet::new();

    let mut files = Vec::new();
    walk(root, &mut files)?;
    files.sort();
    for file in files {
        let relative = file.strip_prefix(root).unwrap_or(&file);
        let mut components = relative.components();
        let Some(package) = components
            .next()
            .map(|value| value.as_os_str().to_string_lossy())
        else {
            continue;
        };
        if components.next().is_none() {
            continue;
        }
        let package = package.to_string();
        let name = file
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if is_code(name) {
            packages.insert(package.clone());
            let text = read_text(&file)?;
            for capture in INCLUDE_RE.captures_iter(&text) {
                if let Some(header) = capture.get(1) {
                    header_packages
                        .entry(header.as_str().to_string())
                        .or_default()
                        .insert(package.clone());
                }
            }
            for call in CALL_RE.find_iter(&text) {
                let Some(name) = NAME_RE.find(call.as_str()) else {
                    continue;
                };
                if names.contains(name.as_str()) {
                    symbol_packages
                        .entry(name.as_str().to_string())
                        .or_default()
                        .insert(package.clone());
                }
            }
        } else if is_build_file(name) {
            packages.insert(package.clone());
            let text = read_text(&file)?;
            collect_libraries(&text, &package, &mut library_packages);
        }
    }

    let libraries = library_packages
        .into_iter()
        .map(|(library, packages)| {
            let stub = STUB_LIBRARIES.contains(&library.as_str());
            (
                library,
                LibraryUse {
                    packages: packages.len(),
                    stub,
                },
            )
        })
        .collect();
    Ok(UsesOutput {
        skipped: None,
        headers: header_packages
            .into_iter()
            .map(|(header, packages)| (header, packages.len()))
            .collect(),
        libraries,
        symbols: symbol_packages
            .into_iter()
            .map(|(name, packages)| (name, packages.len()))
            .collect(),
        undef: None,
        packages: packages.len(),
    })
}

fn collect_libraries(
    text: &str,
    package: &str,
    libraries: &mut BTreeMap<String, BTreeSet<String>>,
) {
    for capture in DASH_L_RE.captures_iter(text) {
        if let Some(library) = capture.get(1) {
            libraries
                .entry(library.as_str().to_string())
                .or_default()
                .insert(package.to_string());
        }
    }
    for capture in CHECK_LIB_RE.captures_iter(text) {
        if let Some(library) = capture.get(1) {
            libraries
                .entry(library.as_str().to_string())
                .or_default()
                .insert(package.to_string());
        }
    }
    for capture in SEARCH_LIBS_RE.captures_iter(text) {
        if let Some(list) = capture.get(1) {
            for library in list.as_str().split_whitespace() {
                if !library.is_empty() {
                    libraries
                        .entry(library.to_string())
                        .or_default()
                        .insert(package.to_string());
                }
            }
        }
    }
}

fn is_code(name: &str) -> bool {
    let Some(extension) = Path::new(name).extension().and_then(|value| value.to_str()) else {
        return false;
    };
    CODE_EXTENSIONS.contains(&extension)
}

fn is_build_file(name: &str) -> bool {
    name.starts_with("Makefile")
        || name.ends_with(".mk")
        || matches!(
            name,
            "configure.ac" | "configure.in" | "CMakeLists.txt" | "meson.build" | "APKBUILD"
        )
        || name.ends_with(".pc")
        || name.ends_with(".pc.in")
}

fn read_text(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn walk(directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
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
        if entry.file_type()?.is_dir() {
            walk(&entry.path(), files)?;
        } else {
            files.push(entry.path());
        }
    }
    Ok(())
}

/// Read `<pkg>.undef` files (S2).
pub fn read_undef(root: &Path) -> Result<UndefOutput> {
    if !root.is_dir() {
        return Ok(UndefOutput {
            skipped: Some("skipped: no undefined-symbol lists".to_string()),
            ..UndefOutput::default()
        });
    }
    let mut strong: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut weak: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut files = Vec::new();
    for entry in fs::read_dir(root).with_context(|| format!("reading {}", root.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) == Some("undef") {
            files.push(path);
        }
    }
    files.sort();
    for path in files {
        let package = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string();
        let text = read_text(&path)?;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((symbol, kind)) = line.split_once('\t') else {
                continue;
            };
            let symbol = symbol.trim();
            if !crate::universe::is_identifier(symbol) {
                continue;
            }
            match kind.trim() {
                "U" => {
                    strong
                        .entry(symbol.to_string())
                        .or_default()
                        .insert(package.clone());
                }
                "w" => {
                    weak.entry(symbol.to_string())
                        .or_default()
                        .insert(package.clone());
                }
                _ => {}
            }
        }
    }
    Ok(UndefOutput {
        skipped: None,
        strong: strong
            .into_iter()
            .map(|(name, packages)| (name, packages.len()))
            .collect(),
        weak: weak
            .into_iter()
            .map(|(name, packages)| (name, packages.len()))
            .collect(),
    })
}

#[cfg(test)]
#[path = "uses_ut.rs"]
mod tests;

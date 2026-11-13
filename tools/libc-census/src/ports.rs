// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Ports snapshot layouts and patch aggregation (ticket C33 section 4.6).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::model::{PackageSummary, PortsOutput, SnapshotSummary};
use crate::patches::{Category, classify_patch};

/// The four optional snapshot directories.
#[derive(Clone, Debug, Default)]
pub struct PortsOptions {
    /// Alpine `aports` root.
    pub aports: Option<PathBuf>,
    /// pkgsrc root.
    pub pkgsrc: Option<PathBuf>,
    /// FreeBSD ports root.
    pub freebsd_ports: Option<PathBuf>,
    /// OpenBSD ports root.
    pub openbsd_ports: Option<PathBuf>,
}

impl PortsOptions {
    /// Whether no snapshot was supplied.
    pub fn is_empty(&self) -> bool {
        self.aports.is_none()
            && self.pkgsrc.is_none()
            && self.freebsd_ports.is_none()
            && self.openbsd_ports.is_none()
    }
}

/// One package located in a snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocatedPackage {
    /// Package id shown in outputs.
    pub id: String,
    /// Snapshot name (`aports`, `pkgsrc`, ...).
    pub snapshot: &'static str,
    /// Package directory.
    pub directory: PathBuf,
    /// Patch files, sorted.
    pub patches: Vec<PathBuf>,
}

/// Locate packages in one snapshot by layout name.
pub fn locate(snapshot: &str, root: &Path) -> Result<Vec<LocatedPackage>> {
    match snapshot {
        "aports" => locate_aports(root),
        "pkgsrc" => locate_category_packages(root, "pkgsrc", "patches", false),
        "freebsd-ports" => locate_category_packages(root, "freebsd-ports", "files", false),
        "openbsd-ports" => locate_category_packages(root, "openbsd-ports", "patches", false),
        other => anyhow::bail!("unknown snapshot `{other}`"),
    }
}

fn locate_aports(root: &Path) -> Result<Vec<LocatedPackage>> {
    let mut packages = BTreeMap::new();
    for repo in sorted_dirs(root)? {
        for package in sorted_dirs(&repo)? {
            let build = package.join("APKBUILD");
            if !build.is_file() {
                continue;
            }
            let Some(id) = package.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            let entry = packages
                .entry(id.to_string())
                .or_insert_with(|| LocatedPackage {
                    id: id.to_string(),
                    snapshot: "aports",
                    directory: package.clone(),
                    patches: Vec::new(),
                });
            for file in sorted_files(&package)? {
                let extension = file.extension().and_then(|value| value.to_str());
                if matches!(extension, Some("patch" | "diff")) {
                    entry.patches.push(file);
                }
            }
        }
    }
    let mut list: Vec<LocatedPackage> = packages.into_values().collect();
    for package in &mut list {
        package.patches.sort();
        package.patches.dedup();
    }
    Ok(list)
}

fn locate_category_packages(
    root: &Path,
    snapshot: &'static str,
    patch_directory: &str,
    aports_layout: bool,
) -> Result<Vec<LocatedPackage>> {
    let mut packages = Vec::new();
    for category in sorted_dirs(root)? {
        for package in sorted_dirs(&category)? {
            let makefile = package.join("Makefile");
            if !makefile.is_file() {
                continue;
            }
            let Some(port) = package.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            let Some(category_name) = category.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            let id = format!("{category_name}/{port}");
            let patch_dir = package.join(patch_directory);
            let mut patches = Vec::new();
            if patch_dir.is_dir() {
                for file in sorted_files(&patch_dir)? {
                    let name = file
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or("");
                    if aports_layout {
                        let extension = file.extension().and_then(|value| value.to_str());
                        if matches!(extension, Some("patch" | "diff")) {
                            patches.push(file);
                        }
                    } else if name.starts_with("patch-") {
                        patches.push(file);
                    }
                }
            }
            packages.push(LocatedPackage {
                id,
                snapshot,
                directory: package,
                patches,
            });
        }
    }
    Ok(packages)
}

fn sorted_dirs(root: &Path) -> Result<Vec<PathBuf>> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in fs::read_dir(root).with_context(|| format!("reading {}", root.display()))? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        if entry.file_type()?.is_dir() {
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}

fn sorted_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(root).with_context(|| format!("reading {}", root.display()))? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}

/// Classify every patch in every supplied snapshot.
pub fn scan(ports: &PortsOptions, symbol_rings: &BTreeMap<String, u8>) -> Result<PortsOutput> {
    let mut output = PortsOutput::default();
    let mut header_packages: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut macro_packages: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut symbol_packages: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for (snapshot, root) in [
        ("aports", &ports.aports),
        ("pkgsrc", &ports.pkgsrc),
        ("freebsd-ports", &ports.freebsd_ports),
        ("openbsd-ports", &ports.openbsd_ports),
    ] {
        let Some(root) = root else {
            output.snapshots.insert(
                snapshot.to_string(),
                SnapshotSummary {
                    packages: 0,
                    skipped: Some(format!("skipped: no {snapshot} snapshot")),
                },
            );
            continue;
        };
        if !root.is_dir() {
            output.snapshots.insert(
                snapshot.to_string(),
                SnapshotSummary {
                    packages: 0,
                    skipped: Some(format!("skipped: {snapshot} directory is missing")),
                },
            );
            continue;
        }
        let packages = locate(snapshot, root)?;
        for package in &packages {
            let mut categories: BTreeSet<Category> = BTreeSet::new();
            let mut headers: BTreeSet<String> = BTreeSet::new();
            let mut macros: BTreeSet<String> = BTreeSet::new();
            let mut symbols: BTreeSet<String> = BTreeSet::new();
            let mut patch_count = 0usize;
            for patch in &package.patches {
                let bytes = fs::read(patch)
                    .with_context(|| format!("reading patch {}", patch.display()))?;
                let text = String::from_utf8_lossy(&bytes);
                let classified = classify_patch(patch, &text, symbol_rings);
                patch_count += 1;
                categories.extend(classified.categories);
                headers.extend(classified.headers);
                macros.extend(classified.macros);
                symbols.extend(classified.symbols);
            }
            let entry = output
                .packages
                .entry(package.id.clone())
                .or_insert_with(PackageSummary::default);
            entry.patches = patch_count;
            entry.categories = categories
                .iter()
                .map(|category| category.as_str().to_string())
                .collect();
            for header in headers {
                header_packages
                    .entry(header)
                    .or_default()
                    .insert(package.id.clone());
            }
            for name in macros {
                macro_packages
                    .entry(name)
                    .or_default()
                    .insert(package.id.clone());
            }
            for name in symbols {
                symbol_packages
                    .entry(name)
                    .or_default()
                    .insert(package.id.clone());
            }
        }
        output.snapshots.insert(
            snapshot.to_string(),
            SnapshotSummary {
                packages: packages.len(),
                skipped: None,
            },
        );
    }

    output.headers = header_packages
        .into_iter()
        .map(|(name, packages)| (name, packages.len()))
        .collect();
    output.macros = macro_packages
        .into_iter()
        .map(|(name, packages)| (name, packages.len()))
        .collect();
    output.symbols = symbol_packages
        .into_iter()
        .map(|(name, packages)| (name, packages.len()))
        .collect();
    Ok(output)
}

/// Categories in the order of ticket 4.6, for the Markdown report.
pub fn category_order() -> [Category; 4] {
    [
        Category::LibcHeader,
        Category::LibcMacro,
        Category::LibcSymbol,
        Category::LibcSemantics,
    ]
}

#[cfg(test)]
#[path = "ports_ut.rs"]
mod tests;

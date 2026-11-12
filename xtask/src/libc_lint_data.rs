// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Parsers for the four libc data files and the provenance fragments.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

/// One row of `xtask/crates.toml`.
#[allow(dead_code)] // The manifest keeps fields later checks will consume.
#[derive(Clone, Debug, Deserialize)]
pub struct CrateRow {
    /// Package name.
    pub name: String,
    /// Path relative to `libc/ravenlibc`.
    pub path: String,
    /// Layer (`L0`..`L3`).
    pub layer: String,
    /// `core` or `alloc`.
    pub std: String,
    /// `P`, `M` or `U`.
    #[serde(rename = "unsafe")]
    pub unsafe_: String,
    /// Raw modules for class M.
    #[serde(default)]
    pub raw_modules: Vec<String>,
    /// `lib` or `bin`.
    #[serde(default = "default_kind")]
    pub kind: String,
    /// Host-compiled flag.
    #[serde(default)]
    pub host: bool,
    /// Alloc feature flag.
    #[serde(default)]
    pub alloc_feature: bool,
    /// Ravenlibc dependencies.
    #[serde(default)]
    pub deps: Vec<String>,
    /// Workspace-crate dependencies.
    #[serde(default)]
    pub external: Vec<String>,
}

fn default_kind() -> String {
    "lib".to_string()
}

/// Parsed `crates.toml`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Crates {
    /// Rows.
    #[serde(default, rename = "crate")]
    pub rows: Vec<CrateRow>,
}

/// One feature row.
#[allow(dead_code)] // The manifest keeps fields later checks will consume.
#[derive(Clone, Debug, Deserialize)]
pub struct Feature {
    /// Feature name.
    pub name: String,
    /// Crates allowed to declare it.
    pub crates: Vec<String>,
}

/// Parsed `features.toml`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Features {
    /// Features.
    #[serde(default, rename = "feature")]
    pub rows: Vec<Feature>,
    /// Custom cfg names.
    #[allow(dead_code)]
    #[serde(default, rename = "cfg")]
    pub cfgs: Vec<Cfg>,
}

/// One custom cfg name.
#[allow(dead_code)] // The manifest keeps fields later checks will consume.
#[derive(Clone, Debug, Deserialize)]
pub struct Cfg {
    /// Name.
    pub name: String,
}

/// One allow-list row.
#[allow(dead_code)] // The manifest keeps fields later checks will consume.
#[derive(Clone, Debug, Deserialize)]
pub struct Allow {
    /// Crate package name.
    #[serde(rename = "crate")]
    pub crate_: String,
    /// File or directory prefix.
    pub path: String,
    /// Non-empty reason.
    pub reason: String,
}

/// Parsed allow-list.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Allowlist {
    /// Rows.
    #[serde(default, rename = "allow")]
    pub rows: Vec<Allow>,
}

/// One tree entry.
#[allow(dead_code)] // The manifest keeps fields later checks will consume.
#[derive(Clone, Debug, Deserialize)]
pub struct TreeEntry {
    /// Relative path.
    pub path: String,
    /// Owning ticket.
    pub owner: String,
    /// Required in this wave.
    #[serde(default)]
    pub required: bool,
    /// `dir` or `file`.
    pub kind: String,
}

/// Parsed `tree.toml`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Tree {
    /// Entries.
    #[serde(default, rename = "entry")]
    pub entries: Vec<TreeEntry>,
    /// Forbidden files.
    #[allow(dead_code)]
    #[serde(default)]
    pub forbidden: Forbidden,
}

/// Forbidden-file list.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Forbidden {
    /// Files relative to `libc/ravenlibc`.
    #[allow(dead_code)]
    #[serde(default)]
    pub files: Vec<String>,
}

/// One `[[import]]` entry from a provenance file.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Import {
    /// Repository-relative path.
    #[serde(default)]
    pub path: String,
    /// Donor name.
    #[allow(dead_code)]
    #[serde(default)]
    pub upstream: String,
    /// Donor file path.
    #[serde(default)]
    pub upstream_path: String,
    /// Donor revision.
    #[allow(dead_code)]
    #[serde(default)]
    pub upstream_rev: String,
    /// SPDX expression.
    #[serde(default)]
    pub license: String,
}

/// Parsed provenance file.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Provenance {
    /// Imports.
    #[serde(default, rename = "import")]
    pub imports: Vec<Import>,
}

fn parse<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parse {}", path.display()))
}

/// Load the four data files from the repository root.
pub fn load(root: &Path) -> Result<(Crates, Features, Allowlist, Tree)> {
    let base = root.join("libc/ravenlibc/xtask");
    Ok((
        parse(&base.join("crates.toml"))?,
        parse(&base.join("features.toml"))?,
        parse(&base.join("unsafe-allowlist.toml"))?,
        parse(&base.join("tree.toml"))?,
    ))
}

/// Load provenance from the root file and every fragment.
pub fn load_provenance(root: &Path) -> Result<Vec<Import>> {
    let mut imports = Vec::new();
    let mut paths = vec![root.join("PROVENANCE.toml")];
    for base in ["libc/ravenlibc", "tests/libc"] {
        collect_fragments(&root.join(base), &mut paths);
    }
    for path in paths {
        if !path.exists() {
            continue;
        }
        if let Ok(parsed) = parse::<Provenance>(&path) {
            imports.extend(parsed.imports);
        } else if path.ends_with("PROVENANCE.toml") {
            return Err(anyhow::anyhow!("parse {}", path.display()));
        }
    }
    Ok(imports)
}

fn collect_fragments(directory: &Path, paths: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_fragments(&path, paths);
        } else if path
            .file_name()
            .is_some_and(|name| name == "provenance.toml")
        {
            paths.push(path);
        }
    }
}

#[cfg(test)]
#[path = "libc_lint_data_ut.rs"]
mod tests;

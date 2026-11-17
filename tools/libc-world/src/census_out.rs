// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `.undef` files in the exact C33 `--undef` input format (section 4.6 S2).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::extract::undef_text;

/// `<results>/<arch>/undef/<name>.undef`.
#[must_use]
pub fn undef_path(results: &Path, arch: &str, name: &str) -> PathBuf {
    results
        .join(arch)
        .join("undef")
        .join(format!("{name}.undef"))
}

/// Write one package's undefined symbols, sorted, with the C33 columns.
pub fn write_undef(
    results: &Path,
    arch: &str,
    name: &str,
    undefined: &BTreeMap<String, char>,
) -> Result<PathBuf> {
    let path = undef_path(results, arch, name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    fs::write(&path, undef_text(undefined))
        .with_context(|| format!("cannot write {}", path.display()))?;
    Ok(path)
}

/// Read one `.undef` file.
pub fn read_undef(path: &Path) -> Result<BTreeMap<String, char>> {
    let text =
        fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    let mut symbols = BTreeMap::new();
    for (name, marker) in parse_c33(&text)? {
        symbols.insert(name, marker);
    }
    Ok(symbols)
}

/// Parse the C33 `--undef` body strictly: `symbol<TAB>U|w`, one per line.
pub fn parse_c33(text: &str) -> Result<Vec<(String, char)>> {
    let mut symbols = Vec::new();
    for (number, line) in text.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() != 2 {
            bail!("line {}: expected two tab-separated fields", number + 1);
        }
        let name = fields[0];
        if name.is_empty() {
            bail!("line {}: empty symbol name", number + 1);
        }
        let marker = match fields[1] {
            "U" => 'U',
            "w" => 'w',
            other => bail!("line {}: unknown type `{other}`", number + 1),
        };
        symbols.push((name.to_string(), marker));
    }
    Ok(symbols)
}

/// The command line a human runs to feed the evidence into the C33 census.
#[must_use]
pub fn census_command(results: &Path, resources: &Path) -> String {
    format!(
        "cargo run -q -p libc-census -- run --undef {} --resources {}",
        results.display(),
        resources.display()
    )
}

#[cfg(test)]
#[path = "census_out_ut.rs"]
mod tests;

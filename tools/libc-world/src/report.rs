// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Result files, the generated dashboard and the ranked missing-symbol backlog (section 4.8).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::census_out::{census_command, read_undef, undef_path};
use crate::classify::SymbolIndex;
use crate::model::{PackageResult, STATUSES};
use crate::world::{PLACEHOLDER, World};

/// Libraries the C33 census ships as empty stubs.
pub const STUB_LIBS: &[&str] = &[
    "m", "pthread", "dl", "rt", "util", "resolv", "crypt", "nsl", "intl", "iconv", "anl", "socket",
    "bsd", "execinfo",
];

/// One aggregated missing symbol.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Aggregate {
    /// Packages that reference it.
    pub packages: u32,
    /// Packages with a strong reference.
    pub strong: u32,
    /// Packages with a weak reference.
    pub weak: u32,
}

/// `<results>/<arch>/<name>.json`.
#[must_use]
pub fn package_result_path(results: &Path, arch: &str, name: &str) -> PathBuf {
    results.join(arch).join(format!("{name}.json"))
}

/// Write one result, byte-identical on a rerun.
pub fn write_result(results: &Path, arch: &str, result: &PackageResult) -> Result<PathBuf> {
    let path = package_result_path(results, arch, &result.name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    let mut text = serde_json::to_string(result)?;
    text.push('\n');
    fs::write(&path, text).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(path)
}

/// Read every package result in one directory, sorted by row number.
pub fn read_results(directory: &Path) -> Result<Vec<PackageResult>> {
    let mut results = Vec::new();
    if !directory.is_dir() {
        return Ok(results);
    }
    let mut files: Vec<PathBuf> = fs::read_dir(directory)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
                && path.file_name().is_some_and(|name| name != "world.json")
        })
        .collect();
    files.sort();
    for file in files {
        let text = fs::read_to_string(&file)?;
        let result: PackageResult = serde_json::from_str(&text)
            .with_context(|| format!("cannot parse {}", file.display()))?;
        results.push(result);
    }
    results.sort_by_key(|result| result.n);
    Ok(results)
}

/// Write the per-package header list.
pub fn write_headers(
    results: &Path,
    arch: &str,
    name: &str,
    headers: &BTreeSet<String>,
) -> Result<()> {
    let directory = results.join(arch).join("headers");
    fs::create_dir_all(&directory)?;
    let mut text = String::new();
    for header in headers {
        text.push_str(header);
        text.push('\n');
    }
    fs::write(directory.join(format!("{name}.txt")), text)?;
    Ok(())
}

/// Aggregate every `.undef` file under the arch directory.
pub fn aggregate_missing(results: &Path, arch: &str) -> Result<BTreeMap<String, Aggregate>> {
    let directory = results.join(arch).join("undef");
    let mut aggregate: BTreeMap<String, Aggregate> = BTreeMap::new();
    if !directory.is_dir() {
        return Ok(aggregate);
    }
    let mut files: Vec<PathBuf> = fs::read_dir(&directory)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "undef")
        })
        .collect();
    files.sort();
    for file in files {
        for (name, marker) in read_undef(&file)? {
            let entry = aggregate.entry(name).or_default();
            entry.packages += 1;
            if marker == 'U' {
                entry.strong += 1;
            } else {
                entry.weak += 1;
            }
        }
    }
    Ok(aggregate)
}

/// Aggregate per-package header lists.
pub fn aggregate_headers(results: &Path, arch: &str) -> Result<BTreeMap<String, u32>> {
    let directory = results.join(arch).join("headers");
    let mut aggregate: BTreeMap<String, u32> = BTreeMap::new();
    if !directory.is_dir() {
        return Ok(aggregate);
    }
    let mut files: Vec<PathBuf> = fs::read_dir(&directory)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "txt"))
        .collect();
    files.sort();
    for file in files {
        let text = fs::read_to_string(&file)?;
        for line in text.lines().filter(|line| !line.is_empty()) {
            *aggregate.entry(line.to_string()).or_default() += 1;
        }
    }
    Ok(aggregate)
}

/// Aggregate libraries named by the results.
#[must_use]
pub fn aggregate_libs(results: &[PackageResult]) -> BTreeMap<String, u32> {
    let mut aggregate: BTreeMap<String, u32> = BTreeMap::new();
    for result in results {
        for library in &result.libs {
            *aggregate.entry(library.clone()).or_default() += 1;
        }
    }
    aggregate
}

/// Whether a library is in the C33 stub set.
#[must_use]
pub fn is_stub_library(name: &str) -> bool {
    STUB_LIBS.contains(&name)
}

/// Render `world.md`.
#[must_use]
pub fn world_md(
    results: &[PackageResult],
    world: &World,
    missing: &BTreeMap<String, Aggregate>,
    headers: &BTreeMap<String, u32>,
    libs: &BTreeMap<String, u32>,
) -> String {
    let mut text = String::new();
    text.push_str("<!-- SPDX-License-Identifier: BSD-3-Clause -->\n");
    text.push_str("<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->\n\n");
    text.push_str("# ravenlibc build-the-world\n\n");
    text.push_str("| status | packages |\n|---|---:|\n");
    for status in STATUSES {
        let count = results
            .iter()
            .filter(|result| result.status == *status)
            .count();
        text.push_str(&format!("| {status} | {count} |\n"));
    }
    let blocked = results
        .iter()
        .filter(|result| result.status.starts_with("blocked-by:"))
        .count();
    if blocked > 0 {
        text.push_str(&format!("| blocked-by | {blocked} |\n"));
    }

    let built: Vec<&PackageResult> = results.iter().filter(|result| result.is_built()).collect();
    let mut weighted = 0.0f64;
    for result in &built {
        if let Some(package) = world.package(&result.name) {
            weighted += if package.optional {
                world.weights.optional
            } else {
                1.0
            };
        }
    }
    text.push_str(&format!(
        "\nWeighted built: {weighted} of {} ({}) of {} packages.\n\n",
        world.weighted_total(),
        built.len(),
        results.len()
    ));

    text.push_str("| n | package | tier | status | failure class | libc patches | platform patches | undefined symbols |\n");
    text.push_str("|---|---|---|---|---|---|---|---|\n");
    let mut sorted = results.to_vec();
    sorted.sort_by_key(|result| result.n);
    for result in &sorted {
        let tier = world
            .package(&result.name)
            .map_or_else(|| "-".to_string(), |package| package.tier.to_string());
        let class = result
            .failure
            .as_ref()
            .map_or_else(|| "-".to_string(), |failure| failure.class.clone());
        text.push_str(&format!(
            "| {} | {} | {tier} | {} | {class} | {} | {} | {} |\n",
            result.n,
            result.name,
            result.status,
            result.libc_patches,
            result.platform_patches,
            result.undefined_symbols
        ));
    }

    let mut top: Vec<(&String, &Aggregate)> = missing.iter().collect();
    top.sort_by(|left, right| {
        right
            .1
            .packages
            .cmp(&left.1.packages)
            .then_with(|| left.0.cmp(right.0))
    });
    text.push_str(
        "\n## Top missing symbols\n\n| symbol | packages | strong | weak |\n|---|---:|---:|---:|\n",
    );
    for (name, counts) in top.iter().take(50) {
        text.push_str(&format!(
            "| {name} | {} | {} | {} |\n",
            counts.packages, counts.strong, counts.weak
        ));
    }

    text.push_str("\n## Headers\n\n| header | packages |\n|---|---:|\n");
    let mut header_rows: Vec<(&String, &u32)> = headers.iter().collect();
    header_rows.sort_by(|left, right| right.1.cmp(left.1).then_with(|| left.0.cmp(right.0)));
    for (header, count) in header_rows.iter().take(50) {
        text.push_str(&format!("| {header} | {count} |\n"));
    }

    text.push_str("\n## Libraries\n\n| lib | packages | kind |\n|---|---:|---|\n");
    let mut lib_rows: Vec<(&String, &u32)> = libs.iter().collect();
    lib_rows.sort_by(|left, right| right.1.cmp(left.1).then_with(|| left.0.cmp(right.0)));
    for (library, count) in lib_rows {
        let kind = if is_stub_library(library) {
            "stub"
        } else {
            "provided"
        };
        text.push_str(&format!("| {library} | {count} | {kind} |\n"));
    }
    text
}

/// Write `world.json`, `world.md` and the three TSV tables.
pub fn write_world(results: &Path, arch: &str, all: &[PackageResult], world: &World) -> Result<()> {
    let directory = results.join(arch);
    fs::create_dir_all(&directory)?;
    let missing = aggregate_missing(results, arch)?;
    let headers = aggregate_headers(results, arch)?;
    let libs = aggregate_libs(all);
    let mut sorted = all.to_vec();
    sorted.sort_by_key(|result| result.n);
    let mut json = serde_json::to_string(&sorted)?;
    json.push('\n');
    fs::write(directory.join("world.json"), json)?;
    fs::write(
        directory.join("world.md"),
        world_md(all, world, &missing, &headers, &libs),
    )?;
    write_tables(results, arch, all)?;
    Ok(())
}

/// Write `missing-symbols.tsv`, `headers.tsv` and `libs.tsv`.
pub fn write_tables(results: &Path, arch: &str, all: &[PackageResult]) -> Result<()> {
    let directory = results.join(arch);
    fs::create_dir_all(&directory)?;
    let missing = aggregate_missing(results, arch)?;
    let mut rows: Vec<(&String, &Aggregate)> = missing.iter().collect();
    rows.sort_by(|left, right| {
        right
            .1
            .packages
            .cmp(&left.1.packages)
            .then_with(|| left.0.cmp(right.0))
    });
    let mut text = String::from("# symbol\tpackages\tstrong\tweak\n");
    for (name, counts) in rows {
        text.push_str(&format!(
            "{name}\t{}\t{}\t{}\n",
            counts.packages, counts.strong, counts.weak
        ));
    }
    fs::write(directory.join("missing-symbols.tsv"), text)?;

    let headers = aggregate_headers(results, arch)?;
    let mut header_rows: Vec<(&String, &u32)> = headers.iter().collect();
    header_rows.sort_by(|left, right| right.1.cmp(left.1).then_with(|| left.0.cmp(right.0)));
    let mut text = String::from("# header\tpackages\n");
    for (header, count) in header_rows {
        text.push_str(&format!("{header}\t{count}\n"));
    }
    fs::write(directory.join("headers.tsv"), text)?;

    let libs = aggregate_libs(all);
    let mut lib_rows: Vec<(&String, &u32)> = libs.iter().collect();
    lib_rows.sort_by(|left, right| right.1.cmp(left.1).then_with(|| left.0.cmp(right.0)));
    let mut text = String::from("# lib\tpackages\tkind\n");
    for (library, count) in lib_rows {
        let kind = if is_stub_library(library) {
            "stub"
        } else {
            "provided"
        };
        text.push_str(&format!("{library}\t{count}\t{kind}\n"));
    }
    fs::write(directory.join("libs.tsv"), text)?;
    Ok(())
}

/// The ranked backlog the `missing` command prints.
pub fn missing_backlog(
    results: &Path,
    arch: &str,
    index: &SymbolIndex,
    resources: &Path,
) -> Result<String> {
    let aggregate = aggregate_missing(results, arch)?;
    let mut rows: Vec<(&String, &Aggregate)> = aggregate.iter().collect();
    rows.sort_by(|left, right| {
        right
            .1
            .packages
            .cmp(&left.1.packages)
            .then_with(|| left.0.cmp(right.0))
    });
    let mut text = String::from("symbol\tpackages\tstrong\tweak\tring\n");
    for (name, counts) in rows {
        let ring = index
            .rings
            .get(name)
            .map_or_else(|| "-".to_string(), u32::to_string);
        text.push_str(&format!(
            "{name}\t{}\t{}\t{}\t{ring}\n",
            counts.packages, counts.strong, counts.weak
        ));
    }
    let undef = results.join(arch).join("undef");
    text.push_str(&format!("\n{}\n", census_command(&undef, resources)));
    Ok(text)
}

/// Whether a result is still a placeholder.
#[must_use]
pub fn is_placeholder(result: &PackageResult) -> bool {
    result.version == PLACEHOLDER || result.status == "placeholder"
}

/// Read one `.undef` file by package name.
pub fn package_undef(results: &Path, arch: &str, name: &str) -> Result<BTreeMap<String, char>> {
    read_undef(&undef_path(results, arch, name))
}

#[cfg(test)]
#[path = "report_ut.rs"]
mod tests;

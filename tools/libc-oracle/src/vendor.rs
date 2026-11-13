// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Vendoring of donor test C (ticket C35, section 4.8; H-10).
//!
//! Accepted files are copied byte-identically and get one `[[import]]` entry each. Anything with a
//! reject phrase, a source file with no recognised licence class, and anything outside the
//! allow-list is refused and listed in `REVIEW.txt`; the O-B6 default is to exclude it.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::gate::REJECT_PHRASES;
use crate::pin;

/// SPDX identifiers the vendoring classifier accepts.
pub(crate) const ALLOWED_SPDX: &[&str] =
    &["BSD-2-Clause", "BSD-3-Clause", "ISC", "MIT", "Apache-2.0"];

/// One vendoring request.
pub(crate) struct VendorOptions<'a> {
    /// Donor name, a key of `donors.lock`.
    pub(crate) donor: &'a str,
    /// Donor directory to walk.
    pub(crate) from: &'a str,
    /// Repository-relative destination directory.
    pub(crate) to: &'a Path,
    /// Copy non-source files when their directory has an accepted source.
    pub(crate) data: bool,
    /// Print the table and write nothing.
    pub(crate) dry_run: bool,
    /// Copy date (`YYYY-MM-DD`), required unless `--dry-run`.
    pub(crate) date: Option<&'a str>,
    /// Donor checkout root.
    pub(crate) resources: &'a Path,
    /// Skip the donor pin check.
    pub(crate) no_pin_check: bool,
}

/// A recognised licence class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Class {
    /// BSD-2-Clause or BSD-3-Clause, decided by the "Neither the name" clause.
    Bsd(&'static str),
    /// ISC.
    Isc,
    /// Public domain.
    Pd,
    /// An allow-listed SPDX identifier.
    Spdx(String),
}

impl Class {
    /// The SPDX identifier written by the vendoring tool.
    pub(crate) fn license(&self) -> &str {
        match self {
            Self::Bsd(license) => license,
            Self::Isc => "ISC",
            Self::Pd => "LicenseRef-PublicDomain",
            Self::Spdx(identifier) => identifier,
        }
    }

    /// Short name used in the dry-run table.
    pub(crate) fn name(&self) -> &str {
        match self {
            Self::Bsd(_) => "bsd",
            Self::Isc => "isc",
            Self::Pd => "pd",
            Self::Spdx(_) => "spdx",
        }
    }
}

/// A file accepted for copying.
struct Accepted {
    relative: String,
    class: Option<Class>,
}

/// Run the vendoring command; returns the process exit code.
pub(crate) fn run(options: &VendorOptions<'_>) -> Result<u8> {
    validate_destination(options.to)?;
    let date = match options.date {
        Some(date) => {
            if !valid_date(date) {
                bail!("--date `{date}` is not YYYY-MM-DD");
            }
            date
        }
        None if options.dry_run => "",
        None => bail!("--date is required unless --dry-run is given"),
    };
    let lock = pin::read_lock(options.resources)?;
    let Some(entry) = lock.get(options.donor) else {
        bail!("donor `{}` is not in donors.lock", options.donor);
    };
    let root = options.resources.join(options.donor).join(options.from);
    if !root.is_dir() {
        bail!("donor directory {} does not exist", root.display());
    }
    let files = walk(&root)?;
    let mut accepted_sources = Vec::new();
    let mut refused = Vec::new();
    let mut accepted_dirs = BTreeSet::new();
    for relative in &files {
        if !is_source(relative) {
            continue;
        }
        let path = root.join(relative);
        let bytes = fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?;
        let text = String::from_utf8_lossy(&bytes);
        match classify(&text) {
            Ok(class) => {
                accepted_sources.push(Accepted {
                    relative: relative.clone(),
                    class: Some(class),
                });
                if let Some(parent) = Path::new(relative).parent() {
                    accepted_dirs.insert(parent.to_path_buf());
                }
            }
            Err(reason) => refused.push((relative.clone(), reason)),
        }
    }

    let mut accepted = accepted_sources;
    let mut ignored = 0usize;
    for relative in &files {
        if is_source(relative) {
            continue;
        }
        let parent = Path::new(relative)
            .parent()
            .map_or_else(PathBuf::new, Path::to_path_buf);
        if options.data && accepted_dirs.contains(&parent) {
            let path = root.join(relative);
            let bytes =
                fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?;
            let text = String::from_utf8_lossy(&bytes);
            accepted.push(Accepted {
                relative: relative.clone(),
                class: classify(&text).ok(),
            });
        } else {
            ignored += 1;
        }
    }

    accepted.sort_by(|left, right| left.relative.cmp(&right.relative));
    refused.sort_by(|left, right| left.0.cmp(&right.0));

    let mut class_counts: BTreeMap<String, usize> = BTreeMap::new();
    for file in &accepted {
        let name = file.class.as_ref().map_or("data", Class::name).to_string();
        *class_counts.entry(name).or_insert(0) += 1;
    }
    for (name, count) in &class_counts {
        println!("class {name}: {count}");
    }
    for (relative, reason) in &refused {
        println!("refused: {relative} ({reason})");
    }
    println!("ignored: {ignored}");
    println!("accepted {}, refused {}", accepted.len(), refused.len());

    if !options.no_pin_check {
        for file in &accepted {
            pin::check(
                options.resources,
                options.donor,
                &format!("{}/{}", options.from, file.relative),
            )?;
        }
    }

    if options.dry_run {
        println!("dry run: nothing written");
        return Ok(0);
    }

    let destination = options.to;
    fs::create_dir_all(destination)
        .with_context(|| format!("cannot create {}", destination.display()))?;
    for file in &accepted {
        let source = root.join(&file.relative);
        let target = destination.join(&file.relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        fs::copy(&source, &target)
            .with_context(|| format!("cannot copy {} to {}", source.display(), target.display()))?;
    }

    let mut review = String::from("# Refused donor files; O-B6 default: excluded.\n");
    for (relative, reason) in &refused {
        review.push_str(&format!("{relative}\t{reason}\n"));
    }
    fs::write(destination.join("REVIEW.txt"), review)
        .with_context(|| format!("cannot write {}", destination.join("REVIEW.txt").display()))?;

    let mut imports = Vec::new();
    for file in &accepted {
        imports.push(Import {
            path: join_relative(destination, &file.relative),
            upstream: options.donor.to_string(),
            upstream_path: format!("{}/{}", options.from, file.relative),
            upstream_rev: entry.sha.clone(),
            license: file
                .class
                .as_ref()
                .map_or_else(|| "NOASSERTION".to_string(), |class| class.license().to_string()),
            tier: "A".to_string(),
            copied: date.to_string(),
            modified: false,
            notes: if file.class.is_some() {
                "vendored test, H-10, never linked into a shipped artifact".to_string()
            } else {
                "vendored data file, licence class not classified, never linked into a shipped artifact"
                    .to_string()
            },
        });
    }
    let provenance = Provenance { import: imports };
    let body = toml::to_string_pretty(&provenance)
        .context("cannot serialise the vendoring provenance fragment")?;
    let text = format!(
        "# SPDX-License-Identifier: BSD-3-Clause\n# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors\n\n{body}"
    );
    fs::write(destination.join("provenance.toml"), text).with_context(|| {
        format!(
            "cannot write {}",
            destination.join("provenance.toml").display()
        )
    })?;
    println!("wrote {}", destination.join("provenance.toml").display());
    Ok(0)
}

#[derive(Serialize)]
struct Provenance {
    import: Vec<Import>,
}

#[derive(Serialize)]
struct Import {
    path: String,
    upstream: String,
    upstream_path: String,
    upstream_rev: String,
    license: String,
    tier: String,
    copied: String,
    modified: bool,
    notes: String,
}

fn validate_destination(to: &Path) -> Result<()> {
    if to.as_os_str().is_empty() {
        bail!("--to is empty");
    }
    if to
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        bail!("--to must not contain `..` or `.`");
    }
    Ok(())
}

fn valid_date(date: &str) -> bool {
    if date.len() != 10 {
        return false;
    }
    let bytes = date.as_bytes();
    if bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let month: u32 = date[5..7].parse().unwrap_or(0);
    let day: u32 = date[8..10].parse().unwrap_or(0);
    date[..4].chars().all(|c| c.is_ascii_digit())
        && (1..=12).contains(&month)
        && (1..=31).contains(&day)
}

fn is_source(relative: &str) -> bool {
    relative.ends_with(".c") || relative.ends_with(".h")
}

fn find_spdx(text: &str) -> Option<String> {
    for line in text.lines() {
        if let Some(index) = line.find("SPDX-License-Identifier:") {
            let rest = line[index + "SPDX-License-Identifier:".len()..].trim();
            let identifier = rest
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_end_matches("*/")
                .trim();
            return Some(identifier.to_string());
        }
    }
    None
}

/// Classify a donor file by its first 80 lines.
pub(crate) fn classify(text: &str) -> std::result::Result<Class, String> {
    for phrase in REJECT_PHRASES {
        if text.contains(phrase) {
            return Err(format!("reject phrase `{phrase}`"));
        }
    }
    let head: String = text.lines().take(80).collect::<Vec<_>>().join("\n");
    if head.contains("Redistribution and use") {
        return Ok(Class::Bsd(if text.contains("Neither the name") {
            "BSD-3-Clause"
        } else {
            "BSD-2-Clause"
        }));
    }
    if head.contains("Permission to use") {
        return Ok(Class::Isc);
    }
    if head.contains("Public domain") || head.contains("public domain") {
        return Ok(Class::Pd);
    }
    if let Some(identifier) = find_spdx(&head) {
        if ALLOWED_SPDX.contains(&identifier.as_str()) {
            return Ok(Class::Spdx(identifier));
        }
        return Err(format!("SPDX `{identifier}` is not allow-listed"));
    }
    Err("no licence class in the first 80 lines".to_string())
}

fn walk(root: &Path) -> Result<Vec<String>> {
    let mut files = Vec::new();
    walk_into(root, root, &mut files)?;
    files.sort();
    Ok(files)
}

fn walk_into(root: &Path, directory: &Path, files: &mut Vec<String>) -> Result<()> {
    let entries =
        fs::read_dir(directory).with_context(|| format!("cannot read {}", directory.display()))?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.with_context(|| format!("cannot read {}", directory.display()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        names.push(name);
    }
    names.sort();
    for name in names {
        let path = directory.join(&name);
        if path.is_dir() {
            walk_into(root, &path, files)?;
        } else if path.is_file() {
            let relative = path
                .strip_prefix(root)
                .expect("walk path under root")
                .to_string_lossy()
                .replace('\\', "/");
            files.push(relative);
        }
    }
    Ok(())
}

fn join_relative(base: &Path, relative: &str) -> String {
    let mut path = base.to_string_lossy().replace('\\', "/");
    if !path.ends_with('/') {
        path.push('/');
    }
    path.push_str(relative);
    path
}

#[cfg(test)]
#[path = "vendor_ut.rs"]
mod tests;

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Man-page coverage and rendering checks.
//!
//! Every program packed into an initfs profile, every directory under `userland/bin`, `init`, and
//! every syscall declared in `crates/raven-abi/src/syscall.rs` must have an mdoc page under
//! `docs/man<N>/`. The scan is deterministic: expected pages are kept in a sorted set and missing
//! paths are printed in sorted order.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::profiles::Profile;

/// One required man page.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Page {
    /// Manual section (`1`, `2`, `4` or `8`).
    pub(crate) section: u8,
    /// Page base name without the section suffix.
    pub(crate) name: String,
}

impl Page {
    /// Path relative to the repository root.
    pub(crate) fn relative_path(&self) -> String {
        format!("docs/man{0}/{1}.{0}", self.section, self.name)
    }
}

/// Run the docs gate. Returns 1 when pages are missing or `mandoc` reports warnings.
pub(crate) fn run(root: &Path) -> Result<u8> {
    let expected = expected_pages(root)?;
    let missing: Vec<String> = expected
        .iter()
        .filter(|page| !root.join(page.relative_path()).is_file())
        .map(Page::relative_path)
        .collect();
    if !missing.is_empty() {
        for path in &missing {
            println!("missing man page: {path}");
        }
        return Ok(1);
    }

    match lint_pages(root)? {
        LintOutcome::Skipped => {
            println!("docs: mandoc not installed; rendering check skipped");
            Ok(0)
        }
        LintOutcome::Checked { checked, failures } if failures.is_empty() => {
            println!("docs: {checked} page(s) checked");
            Ok(0)
        }
        LintOutcome::Checked { failures, .. } => {
            for failure in failures {
                println!("{failure}");
            }
            Ok(1)
        }
    }
}

/// Expected pages for the current tree, sorted by section and name.
pub(crate) fn expected_pages(root: &Path) -> Result<BTreeSet<Page>> {
    let mut pages = BTreeSet::new();

    let profiles = root.join("profiles");
    if profiles.is_dir() {
        let mut names = Vec::new();
        for entry in fs::read_dir(&profiles)
            .with_context(|| format!("cannot list profile directory {}", profiles.display()))?
        {
            let entry = entry.with_context(|| format!("cannot read {}", profiles.display()))?;
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) == Some("toml") {
                let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) else {
                    bail!("profile {} has no valid file name", path.display());
                };
                names.push(name.to_string());
            }
        }
        names.sort();
        for name in names {
            let profile = Profile::load(root, &name)?;
            for program in &profile.initfs.programs {
                let page_name = program
                    .package
                    .strip_prefix("raven-")
                    .unwrap_or(&program.package);
                let section = if page_name == "init" || program.path.starts_with("/sbin/") {
                    8
                } else {
                    1
                };
                pages.insert(Page {
                    section,
                    name: page_name.to_string(),
                });
            }
        }
    }

    let bin = root.join("userland/bin");
    if bin.is_dir() {
        let mut names = Vec::new();
        for entry in fs::read_dir(&bin).with_context(|| format!("cannot list {}", bin.display()))? {
            let entry = entry.with_context(|| format!("cannot read {}", bin.display()))?;
            let path = entry.path();
            if path.is_dir() && path.join("Cargo.toml").is_file() {
                let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                    bail!("binary directory {} has no valid name", path.display());
                };
                names.push(name.to_string());
            }
        }
        names.sort();
        for name in names {
            pages.insert(Page { section: 1, name });
        }
    }

    if root.join("userland/init").is_dir() {
        pages.insert(Page {
            section: 8,
            name: "init".to_string(),
        });
    }

    let syscalls = syscall_names(root)?;
    for name in syscalls {
        pages.insert(Page {
            section: 2,
            name: name.to_ascii_lowercase(),
        });
    }

    Ok(pages)
}

fn syscall_names(root: &Path) -> Result<Vec<String>> {
    let path = root.join("crates/raven-abi/src/syscall.rs");
    let text = fs::read_to_string(&path)
        .with_context(|| format!("cannot read syscall table {}", path.display()))?;
    let mut names = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some((name, rest)) = line.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        {
            continue;
        }
        let rest = rest.trim();
        if rest.starts_with("0x") && rest.ends_with(';') {
            names.push(name.to_string());
        }
    }
    names.sort();
    names.dedup();
    Ok(names)
}

enum LintOutcome {
    Skipped,
    Checked {
        checked: usize,
        failures: Vec<String>,
    },
}

fn lint_pages(root: &Path) -> Result<LintOutcome> {
    let mut pages = Vec::new();
    for section in [1u8, 2, 4, 8] {
        let directory = root.join(format!("docs/man{section}"));
        if !directory.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&directory)
            .with_context(|| format!("cannot list {}", directory.display()))?
        {
            let entry = entry.with_context(|| format!("cannot read {}", directory.display()))?;
            let path = entry.path();
            if path.is_file() {
                pages.push(path);
            }
        }
    }
    pages.sort();

    let mut checked = 0usize;
    let mut failures = Vec::new();
    for page in pages {
        let output = match Command::new("mandoc")
            // Gate 6.2 uses `-Wwarning`: STYLE findings are advisory, while WARNING/ERROR
            // findings (real markup defects) still fail the page.
            .args(["-T", "lint", "-W", "warning"])
            .arg(&page)
            .output()
        {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(LintOutcome::Skipped);
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("cannot run mandoc on {}", page.display()));
            }
        };
        checked += 1;
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !output.status.success() || !stderr.trim().is_empty() {
            let relative = page
                .strip_prefix(root)
                .unwrap_or(&page)
                .display()
                .to_string();
            let detail = stderr.trim();
            if detail.is_empty() {
                failures.push(format!(
                    "mandoc: {} failed with {}",
                    relative, output.status
                ));
            } else {
                failures.push(format!("mandoc: {relative}: {detail}"));
            }
        }
    }
    Ok(LintOutcome::Checked { checked, failures })
}

#[cfg(test)]
#[path = "docs_ut.rs"]
mod tests;

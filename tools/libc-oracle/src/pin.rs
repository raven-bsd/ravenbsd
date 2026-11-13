// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Donor pin checks (ticket C35, section 4.4.1).
//!
//! `donors.lock` pins each checkout to a commit. Before a donor file is compiled, the checkout
//! `HEAD` must equal the pin and the file must be unmodified. `--no-pin-check` is for fixtures and
//! non-git trees only.

use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::failure::Failure;

/// One row of `donors.lock`.
#[derive(Clone, Debug)]
pub(crate) struct LockEntry {
    /// Pinned commit (40 lowercase hex characters).
    pub(crate) sha: String,
}

/// Read `donors.lock` as a map keyed by donor name.
pub(crate) fn read_lock(resources: &Path) -> Result<BTreeMap<String, LockEntry>> {
    let path = resources.join("donors.lock");
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read donor lock {}", path.display()))?;
    let mut entries = BTreeMap::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 2 {
            bail!(
                "{}:{}: donor lock row needs at least a name and a sha",
                path.display(),
                number + 1
            );
        }
        let sha = fields[1];
        if sha.len() != 40 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
            bail!(
                "{}:{}: `{sha}` is not a 40-hex commit sha",
                path.display(),
                number + 1
            );
        }
        entries.insert(
            fields[0].to_string(),
            LockEntry {
                sha: sha.to_string(),
            },
        );
    }
    Ok(entries)
}

/// Check that `donor` is at its pin and `relative` is unmodified.
pub(crate) fn check(resources: &Path, donor: &str, relative: &str) -> Result<()> {
    if !git_available() {
        return Err(Failure::skipped("git"));
    }
    let lock = read_lock(resources)?;
    let Some(entry) = lock.get(donor) else {
        return Err(Failure::refused(format!(
            "donor pin: `{donor}` is not in donors.lock"
        )));
    };
    let checkout = resources.join(donor);
    let head = git(&checkout, &["rev-parse", "HEAD"])?;
    if head != entry.sha {
        return Err(Failure::refused(format!(
            "donor pin: {donor} HEAD is {head}, donors.lock pins {}",
            entry.sha
        )));
    }
    let status = git(&checkout, &["status", "--porcelain", "--", relative])?;
    if !status.is_empty() {
        return Err(Failure::refused(format!(
            "donor pin: {donor}:{relative} is modified ({})",
            status.lines().next().unwrap_or("dirty")
        )));
    }
    Ok(())
}

/// Whether a usable `git` binary is on `PATH`.
pub(crate) fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn git(directory: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(args)
        .output();
    match output {
        Err(error) if error.kind() == ErrorKind::NotFound => Err(Failure::skipped("git")),
        Err(error) => {
            Err(error).with_context(|| format!("cannot run git in {}", directory.display()))
        }
        Ok(output) if !output.status.success() => bail!(
            "git {} failed in {}: {}",
            args.join(" "),
            directory.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ),
        Ok(output) => Ok(String::from_utf8_lossy(&output.stdout).trim().to_string()),
    }
}

#[cfg(test)]
#[path = "pin_ut.rs"]
mod tests;
